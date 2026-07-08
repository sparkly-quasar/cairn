<!-- SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0 -->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    detectSystem,
    getRecommendation,
    getCatalog,
    getBundles,
    isModelPresent,
    installOllama,
    pullModel,
    ensureOpenWebui,
    serverStatus,
    setServerTier,
    qrSvg,
    openChat,
    onProgress,
    type SystemProfile,
    type Recommendation,
    type RatedModel,
    type Bundle,
    type ServerStatus,
    type BindTier,
  } from "$lib/api";

  type Mode = "simple" | "explore" | "remote";
  type Step = "welcome" | "specs" | "recommend";
  type Phase = "browse" | "setup" | "done";
  type TaskStatus = "pending" | "active" | "done" | "skipped" | "error";

  const BENCH_URL = "https://lmarena.ai/leaderboard";

  let mode = $state<Mode>("simple");
  let step = $state<Step>("welcome");
  let phase = $state<Phase>("browse");

  let profile = $state<SystemProfile | null>(null);
  let rec = $state<Recommendation | null>(null);
  let loading = $state(false);

  // Explore state
  let bundles = $state<Bundle[]>([]);
  let catalog = $state<RatedModel[]>([]);
  let installed = $state<Set<string>>(new Set());
  let activeBundle = $state<string | null>(null);
  let exploreLoaded = $state(false);
  let exploreLoading = $state(false);
  let ackModel = $state<RatedModel | null>(null);

  // Remote-access state
  let srv = $state<ServerStatus | null>(null);
  let srvLoading = $state(false);
  let srvApplying = $state(false);
  let qrMarkup = $state<string | null>(null);

  // Install pipeline state
  let target = $state<{ tag: string; name: string } | null>(null);
  let returnTo = $state<Mode>("simple");
  let setupError = $state<string | null>(null);
  let chatUrl = $state<string | null>(null);
  let log = $state<string[]>([]);

  let tasks = $state<{ key: string; label: string; status: TaskStatus }[]>([
    { key: "ollama", label: "Prepare the engine (Ollama)", status: "pending" },
    { key: "model", label: "Download your model", status: "pending" },
    { key: "webui", label: "Launch the chat app", status: "pending" },
  ]);

  function setTask(key: string, status: TaskStatus) {
    tasks = tasks.map((t) => (t.key === key ? { ...t, status } : t));
  }

  onMount(() => {
    const unlisteners = [
      onProgress("ollama-install-progress", pushLog),
      onProgress("pull-progress", pushLog),
    ];
    return () => unlisteners.forEach((p) => p.then((u) => u()));
  });

  function pushLog(line: string) {
    if (!line.trim()) return;
    log = [...log.slice(-200), line];
  }

  async function ensureProfile(): Promise<SystemProfile> {
    if (!profile) profile = await detectSystem();
    return profile;
  }

  // ---- Simple mode ----
  async function goSpecs() {
    step = "specs";
    loading = true;
    try {
      profile = await detectSystem();
    } finally {
      loading = false;
    }
  }

  async function goRecommend() {
    step = "recommend";
    loading = true;
    try {
      rec = await getRecommendation();
    } finally {
      loading = false;
    }
  }

  // ---- Explore mode ----
  async function enterExplore() {
    mode = "explore";
    if (exploreLoaded || exploreLoading) return;
    exploreLoading = true;
    try {
      await ensureProfile();
      const [b, c] = await Promise.all([getBundles(), getCatalog()]);
      bundles = b;
      catalog = c;
      const present = await Promise.all(
        c.map(async (m) => [m.ollama_tag, await isModelPresent(m.ollama_tag)] as const),
      );
      installed = new Set(present.filter(([, p]) => p).map(([tag]) => tag));
      exploreLoaded = true;
    } finally {
      exploreLoading = false;
    }
  }

  function enterSimple() {
    mode = "simple";
  }

  // ---- Remote access ----
  async function enterRemote() {
    mode = "remote";
    srvLoading = true;
    try {
      srv = await serverStatus();
      await refreshQr();
    } finally {
      srvLoading = false;
    }
  }

  /** URL other devices use for the current tier (null for Private / not running). */
  function remoteUrl(s: ServerStatus | null): string | null {
    if (!s || !s.running) return null;
    if (s.tier === "lan") return s.lan_url;
    if (s.tier === "tailscale") return s.tailscale_url;
    return null;
  }

  async function refreshQr() {
    const url = remoteUrl(srv);
    qrMarkup = url ? await qrSvg(url) : null;
  }

  async function chooseTier(tier: BindTier) {
    if (!srv || srv.tier === tier || srvApplying) return;
    srvApplying = true;
    qrMarkup = null;
    try {
      srv = await setServerTier(tier);
      await refreshQr();
    } finally {
      srvApplying = false;
    }
  }

  const tierCards: { id: BindTier; icon: string; title: string; who: string }[] = [
    { id: "private", icon: "🔒", title: "Private", who: "Only this computer" },
    { id: "lan", icon: "🏠", title: "Local network", who: "Devices on your Wi-Fi" },
    { id: "tailscale", icon: "🌐", title: "Tailscale", who: "Your devices, anywhere (encrypted)" },
  ];

  const visibleModels = $derived(
    activeBundle ? catalog.filter((m) => m.use_cases.includes(activeBundle!)) : catalog,
  );

  function requestInstall(m: RatedModel) {
    if (m.requires_ack) {
      ackModel = m;
    } else {
      startInstall(m.ollama_tag, m.display_name);
    }
  }

  function confirmAck() {
    if (ackModel) {
      const m = ackModel;
      ackModel = null;
      startInstall(m.ollama_tag, m.display_name);
    }
  }

  // ---- Shared install pipeline ----
  function startInstall(tag: string, name: string) {
    target = { tag, name };
    returnTo = mode;
    runSetup();
  }

  async function runSetup() {
    if (!target) return;
    phase = "setup";
    setupError = null;
    log = [];
    tasks.forEach((t) => setTask(t.key, "pending"));

    try {
      const p = await ensureProfile();

      // 1 — Ollama engine
      if (p.ollama_present) {
        setTask("ollama", "skipped");
      } else {
        setTask("ollama", "active");
        await installOllama();
        setTask("ollama", "done");
      }

      // 2 — Model
      setTask("model", "active");
      const present = await isModelPresent(target.tag);
      if (present) {
        pushLog(`${target.name} is already downloaded.`);
      } else {
        await pullModel(target.tag);
      }
      installed = new Set([...installed, target.tag]);
      setTask("model", "done");

      // 3 — Open WebUI
      setTask("webui", "active");
      chatUrl = await ensureOpenWebui();
      setTask("webui", "done");

      phase = "done";
    } catch (err) {
      const active = tasks.find((t) => t.status === "active");
      if (active) setTask(active.key, "error");
      setupError = typeof err === "string" ? err : String(err);
    }
  }

  function finishInstall() {
    phase = "browse";
    mode = returnTo;
  }

  // ---- display helpers ----
  const vendorLabel: Record<string, string> = {
    apple: "Apple Silicon (unified memory)",
    nvidia: "NVIDIA GPU",
    amd: "AMD GPU",
    none: "No dedicated GPU",
  };
  const capMeta: Record<string, { icon: string; label: string }> = {
    chat: { icon: "💬", label: "Chat" },
    reasoning: { icon: "🧠", label: "Reasoning" },
    code: { icon: "💻", label: "Code" },
    vision: { icon: "👁️", label: "Vision" },
    medical: { icon: "🩺", label: "Medical" },
    multilingual: { icon: "🌍", label: "Multilingual" },
    fast: { icon: "⚡", label: "Fast (MoE)" },
    conversational: { icon: "🗣️", label: "Conversational" },
  };
  const gb = (n: number) => `${n.toFixed(n < 10 ? 1 : 0)} GB`;
  const ratingClass = (r?: string) => (r === "green" ? "ok" : r === "yellow" ? "warn" : "bad");
  const statusIcon = (s: TaskStatus) =>
    s === "done" ? "✓" : s === "skipped" ? "–" : s === "error" ? "✕" : s === "active" ? "…" : "";
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && ackModel && (ackModel = null)} />

<main>
  <header class="brand">
    <div class="cairn" aria-hidden="true">
      <span></span><span></span><span></span>
    </div>
    <h1>Cairn</h1>
    <p class="tagline">Your own AI, running privately on this computer.</p>
  </header>

  {#if phase === "browse"}
    <nav class="modes" aria-label="Mode">
      <button class:active={mode === "simple"} onclick={enterSimple}>Simple</button>
      <button class:active={mode === "explore"} onclick={enterExplore}>Explore</button>
      <button class:active={mode === "remote"} onclick={enterRemote}>Remote</button>
    </nav>
  {/if}

  <!-- ============ SIMPLE MODE ============ -->
  {#if phase === "browse" && mode === "simple"}
    {#if step === "welcome"}
      <section class="card">
        <h2>Let's set up your private assistant</h2>
        <p>
          Cairn installs a local AI model and a chat app on your machine. Nothing you type
          leaves your computer — it runs entirely offline once set up. This takes a few minutes.
        </p>
        <button class="primary" onclick={goSpecs}>Get started</button>
        <button class="ghost" onclick={enterExplore}>Or browse all models</button>
      </section>
    {/if}

    {#if step === "specs"}
      <section class="card">
        <h2>Your computer</h2>
        {#if loading || !profile}
          <p class="muted">Checking your hardware…</p>
        {:else}
          <ul class="specs">
            <li><span>Chip</span><strong>{profile.gpu_name ?? profile.arch}</strong></li>
            <li><span>Memory</span><strong>{gb(profile.ram_gb)}</strong></li>
            <li><span>Graphics</span><strong>{vendorLabel[profile.gpu_vendor]}{profile.gpu_experimental ? " (experimental)" : ""}</strong></li>
            <li><span>Free disk</span><strong>{gb(profile.free_disk_gb)}</strong></li>
            <li><span>Engine (Ollama)</span><strong class={profile.ollama_present ? "ok" : "warn"}>{profile.ollama_present ? "Ready" : "Will install"}</strong></li>
            <li><span>Docker</span><strong class={profile.docker_present ? "ok" : "bad"}>{profile.docker_present ? "Ready" : "Not found"}</strong></li>
          </ul>
          {#if !profile.docker_present}
            <p class="notice bad">Docker isn't installed. Cairn needs Docker Desktop to run the chat app. Install it from docker.com, then reopen Cairn.</p>
          {/if}
          <button class="primary" onclick={goRecommend} disabled={!profile.docker_present}>Continue</button>
        {/if}
      </section>
    {/if}

    {#if step === "recommend"}
      <section class="card">
        <h2>Recommended for your machine</h2>
        {#if loading || !rec}
          <p class="muted">Picking the best model…</p>
        {:else}
          <div class="model">
            <div class="model-head">
              <strong>{rec.display_name}</strong>
              <span class="badge {ratingClass(rec.rating)}">{rec.rating_label}</span>
            </div>
            <p>{rec.reason}</p>
            <p class="muted small">Download size ≈ {gb(rec.disk_gb)}</p>
          </div>
          <button class="primary" onclick={() => rec && startInstall(rec.ollama_tag, rec.display_name)}>Set up my assistant</button>
          <button class="ghost" onclick={goSpecs}>Back</button>
        {/if}
      </section>
    {/if}
  {/if}

  <!-- ============ EXPLORE MODE ============ -->
  {#if phase === "browse" && mode === "explore"}
    <section class="card wide">
      <div class="explore-head">
        <h2>Model catalog</h2>
        <button class="link" onclick={() => openChat(BENCH_URL)}>Compare models ↗</button>
      </div>
      <p class="muted small">
        Each model is rated for your machine —
        <span class="ok">green</span> fits comfortably,
        <span class="warn">yellow</span> runs slower,
        <span class="bad">red</span> is likely too big.
      </p>

      {#if exploreLoading || !exploreLoaded}
        <p class="muted">Rating models for your hardware…</p>
      {:else}
        <div class="chips" role="tablist">
          <button class="chip" class:active={activeBundle === null} onclick={() => (activeBundle = null)}>All</button>
          {#each bundles as b (b.id)}
            <button class="chip" class:active={activeBundle === b.id} onclick={() => (activeBundle = b.id)}>
              {b.icon} {b.title}
            </button>
          {/each}
        </div>

        {#if activeBundle}
          {@const bundle = bundles.find((b) => b.id === activeBundle)}
          {#if bundle}<p class="bundle-blurb muted small">{bundle.blurb}</p>{/if}
        {/if}

        {#if !profile?.docker_present}
          <p class="notice bad">Docker isn't running. Installs are disabled until Docker Desktop is available.</p>
        {/if}

        <div class="grid">
          {#each visibleModels as m (m.id)}
            <article class="mcard {ratingClass(m.rating)}">
              <div class="mcard-head">
                <div>
                  <strong>{m.display_name}</strong>
                  <span class="muted small">{m.params} · ≈ {gb(m.disk_gb)}</span>
                </div>
                <span class="badge {ratingClass(m.rating)}" title={m.reason}>{m.rating_label}</span>
              </div>

              <div class="caps">
                {#each m.capabilities as c}
                  <span class="cap">{capMeta[c]?.icon ?? ""} {capMeta[c]?.label ?? c}</span>
                {/each}
              </div>

              <p class="mblurb">{m.blurb}</p>

              <div class="mcard-foot">
                {#if installed.has(m.ollama_tag)}
                  <button class="primary small-btn" onclick={() => startInstall(m.ollama_tag, m.display_name)}>Installed · Open</button>
                {:else}
                  <button class="primary small-btn" disabled={!profile?.docker_present} onclick={() => requestInstall(m)}>
                    {m.requires_ack ? "Install…" : "Install"}
                  </button>
                {/if}
                <button class="link" onclick={() => openChat(m.library_url)}>Details ↗</button>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- ============ REMOTE ACCESS ============ -->
  {#if phase === "browse" && mode === "remote"}
    <section class="card">
      <h2>Remote access</h2>
      {#if srvLoading || !srv}
        <p class="muted">Checking your server…</p>
      {:else if !srv.running}
        <p>
          Your assistant isn't running yet. Set it up first, then come back here to
          make it reachable from your other devices.
        </p>
        <button class="primary" onclick={() => { mode = "simple"; step = "welcome"; }}>Go to setup</button>
      {:else}
        <p class="muted small">
          By default your AI is private to this computer. Choose who else can reach it:
        </p>

        <div class="tiers">
          {#each tierCards as t (t.id)}
            <button
              class="tier"
              class:active={srv.tier === t.id}
              disabled={srvApplying}
              onclick={() => chooseTier(t.id)}
            >
              <span class="tier-icon">{t.icon}</span>
              <span class="tier-body">
                <strong>{t.title}</strong>
                <span class="muted small">{t.who}</span>
              </span>
              {#if srv.tier === t.id}<span class="tier-dot">●</span>{/if}
            </button>
          {/each}
        </div>

        {#if srvApplying}
          <p class="muted small">Applying — restarting the chat server…</p>
        {/if}

        <!-- Connection details for the active tier -->
        {#if srv.tier === "private"}
          <div class="conn">
            <p>Reachable only on this computer.</p>
            {#if srv.private_url}<p class="muted small">Open at {srv.private_url}</p>{/if}
          </div>
        {:else if srv.tier === "lan"}
          {#if srv.lan_url}
            <div class="conn">
              <p class="url">{srv.lan_url}</p>
              <p class="muted small">Any device on your Wi-Fi can open this address.</p>
              {#if qrMarkup}<div class="qr">{@html qrMarkup}</div>{/if}
            </div>
          {:else}
            <p class="notice bad">Couldn't determine your local network address.</p>
          {/if}
        {:else if srv.tier === "tailscale"}
          {#if !srv.tailscale.installed}
            <p class="notice bad">
              Tailscale isn't installed. Install it from tailscale.com, sign in, then reopen this tab.
            </p>
          {:else if !srv.tailscale.running}
            <p class="notice bad">
              Tailscale is installed but not connected. Run <code>tailscale up</code> and sign in, then reopen this tab.
            </p>
          {:else if srv.tailscale_url}
            <div class="conn">
              <p class="url">{srv.tailscale_url}</p>
              <p class="muted small">Reachable from any of your Tailscale devices, encrypted end-to-end.</p>
              {#if qrMarkup}<div class="qr">{@html qrMarkup}</div>{/if}
            </div>
          {/if}
        {/if}

        {#if srv.tier !== "private"}
          <p class="notice warn-box">
            Anyone who can reach this address can use your assistant. Make sure you've created a
            password in Open WebUI — the first account you register becomes the admin.
          </p>
        {/if}

        {#if remoteUrl(srv)}
          <button class="ghost" onclick={() => { const u = remoteUrl(srv); if (u) openChat(u); }}>Open this address here</button>
        {/if}
      {/if}
    </section>
  {/if}

  <!-- ============ SHARED: SETUP / DONE ============ -->
  {#if phase === "setup"}
    <section class="card">
      <h2>Setting up {target?.name ?? "your model"}</h2>
      <ol class="tasks">
        {#each tasks as t (t.key)}
          <li class={t.status}>
            <span class="tick">{statusIcon(t.status)}</span>
            <span class="tlabel">{t.label}</span>
            {#if t.status === "skipped"}<span class="muted small">already done</span>{/if}
          </li>
        {/each}
      </ol>

      {#if log.length}
        <pre class="log">{log.join("\n")}</pre>
      {/if}

      {#if setupError}
        <p class="notice bad">{setupError}</p>
        <button class="primary" onclick={runSetup}>Retry</button>
        <button class="ghost" onclick={finishInstall}>Cancel</button>
      {/if}
    </section>
  {/if}

  {#if phase === "done"}
    <section class="card success">
      <div class="check">✓</div>
      <h2>You're all set</h2>
      <p>{target?.name ?? "Your assistant"} is ready. Click below to start chatting in your browser.</p>
      <button class="primary" onclick={() => chatUrl && openChat(chatUrl)}>Open my assistant</button>
      {#if chatUrl}<p class="muted small">Running at {chatUrl}</p>{/if}
      <button class="ghost" onclick={finishInstall}>{returnTo === "explore" ? "Back to catalog" : "Done"}</button>
    </section>
  {/if}

  <!-- ============ ACKNOWLEDGMENT MODAL ============ -->
  {#if ackModel}
    <div class="scrim" role="presentation" onclick={() => (ackModel = null)} onkeydown={(e) => e.key === "Escape" && (ackModel = null)}>
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="ack-title" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
        <h2 id="ack-title">{ackModel.requires_ack?.headline}</h2>
        <p class="muted small">Before installing {ackModel.display_name}, please read and accept:</p>
        <ul class="ack">
          {#each ackModel.requires_ack?.points ?? [] as pt}
            <li>{pt}</li>
          {/each}
        </ul>
        <button class="primary" onclick={confirmAck}>I understand — install anyway</button>
        <button class="ghost" onclick={() => (ackModel = null)}>Cancel</button>
      </div>
    </div>
  {/if}

  <footer>Local-first · Apache-2.0 · Cairn</footer>
</main>

<style>
  :global(:root) {
    --bg: #f4f2ee;
    --card: #fffdf9;
    --ink: #23201b;
    --muted: #7a7266;
    --line: #e6e1d8;
    --accent: #3f6f5b;
    --accent-ink: #ffffff;
    --ok: #2f7d5b;
    --warn: #a5741a;
    --bad: #b23b34;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #1c1a17;
      --card: #262320;
      --ink: #ece7df;
      --muted: #9c9285;
      --line: #3a3530;
      --accent: #5a9b81;
      --accent-ink: #10140f;
      --ok: #6bbf95;
      --warn: #d6a24e;
      --bad: #e07b73;
    }
  }

  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--ink);
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  main {
    max-width: 820px;
    margin: 0 auto;
    padding: 2.4rem 1.5rem 1.5rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    min-height: 100vh;
    box-sizing: border-box;
  }

  .brand { text-align: center; margin-bottom: 1rem; }
  .cairn { display: flex; flex-direction: column; align-items: center; gap: 3px; margin-bottom: 0.5rem; }
  .cairn span {
    display: block; background: var(--accent); border-radius: 40%;
  }
  .cairn span:nth-child(1) { width: 20px; height: 12px; }
  .cairn span:nth-child(2) { width: 32px; height: 14px; }
  .cairn span:nth-child(3) { width: 44px; height: 16px; }
  h1 { margin: 0.2rem 0 0; font-size: 1.9rem; letter-spacing: -0.02em; }
  .tagline { margin: 0.3rem 0 0; color: var(--muted); }

  .modes {
    display: inline-flex; gap: 4px; margin-bottom: 1.4rem;
    background: var(--card); border: 1px solid var(--line);
    border-radius: 999px; padding: 4px;
  }
  .modes button {
    border: none; background: transparent; color: var(--muted);
    padding: 0.4rem 1.1rem; border-radius: 999px; font: inherit; font-weight: 600;
    cursor: pointer;
  }
  .modes button.active { background: var(--accent); color: var(--accent-ink); }

  .card {
    width: 100%;
    max-width: 560px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 16px;
    padding: 1.6rem;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.05);
    box-sizing: border-box;
  }
  .card.wide { max-width: 100%; }
  h2 { margin: 0 0 0.7rem; font-size: 1.25rem; }
  p { line-height: 1.55; }
  .muted { color: var(--muted); }
  .small { font-size: 0.85rem; }

  button {
    font: inherit; font-weight: 600; cursor: pointer;
    border-radius: 10px; padding: 0.8rem 1.2rem; border: 1px solid transparent;
    transition: transform 0.05s ease, filter 0.15s ease;
  }
  button:active { transform: translateY(1px); }
  .primary {
    display: block; width: 100%; margin-top: 1.3rem;
    background: var(--accent); color: var(--accent-ink);
  }
  .primary:hover { filter: brightness(1.06); }
  .primary:disabled { opacity: 0.45; cursor: not-allowed; }
  .ghost {
    display: block; width: 100%; margin-top: 0.6rem;
    background: transparent; color: var(--muted); border-color: var(--line);
  }
  .link {
    background: none; border: none; color: var(--accent);
    padding: 0; font-weight: 600; cursor: pointer;
  }
  .link:hover { text-decoration: underline; }

  .specs { list-style: none; padding: 0; margin: 0.5rem 0 0; }
  .specs li {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.6rem 0; border-bottom: 1px solid var(--line);
  }
  .specs li:last-child { border-bottom: none; }
  .specs span { color: var(--muted); }

  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .bad { color: var(--bad); }

  .model-head { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.3rem; }
  .model-head strong { font-size: 1.15rem; }
  .badge {
    font-size: 0.75rem; font-weight: 700; padding: 0.15rem 0.55rem; border-radius: 999px;
    border: 1px solid currentColor; white-space: nowrap;
  }
  .badge.ok { color: var(--ok); }
  .badge.warn { color: var(--warn); }
  .badge.bad { color: var(--bad); }

  .notice { border-radius: 10px; padding: 0.7rem 0.9rem; font-size: 0.92rem; margin-top: 0.8rem; }
  .notice.bad { background: color-mix(in srgb, var(--bad) 12%, transparent); }
  .notice.warn-box { background: color-mix(in srgb, var(--warn) 14%, transparent); }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.85em;
    background: color-mix(in srgb, var(--ink) 8%, transparent); padding: 0.1rem 0.35rem; border-radius: 5px;
  }

  /* ---- Remote access ---- */
  .tiers { display: flex; flex-direction: column; gap: 0.6rem; margin: 1rem 0; }
  .tier {
    display: flex; align-items: center; gap: 0.8rem; text-align: left; width: 100%;
    background: transparent; border: 1px solid var(--line); border-radius: 12px; padding: 0.8rem 1rem;
  }
  .tier.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .tier:disabled { opacity: 0.55; cursor: progress; }
  .tier-icon { font-size: 1.4rem; }
  .tier-body { display: flex; flex-direction: column; }
  .tier-body strong { font-size: 1rem; }
  .tier-dot { margin-left: auto; color: var(--accent); }

  .conn { margin-top: 0.8rem; }
  .conn .url {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-weight: 600;
    word-break: break-all; margin: 0 0 0.2rem;
  }
  .qr {
    width: 200px; max-width: 100%; margin: 0.9rem auto 0; padding: 12px;
    background: #ffffff; border-radius: 10px; box-sizing: border-box;
  }
  .qr :global(svg) { display: block; width: 100%; height: auto; }

  /* ---- Explore ---- */
  .explore-head { display: flex; justify-content: space-between; align-items: baseline; gap: 1rem; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.5rem; margin: 1rem 0 0.4rem; }
  .chip {
    background: transparent; border: 1px solid var(--line); color: var(--muted);
    padding: 0.4rem 0.8rem; border-radius: 999px; font-size: 0.85rem; font-weight: 600;
  }
  .chip.active { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
  .bundle-blurb { margin: 0.4rem 0 0; }

  .grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 0.9rem; margin-top: 1rem;
  }
  .mcard {
    border: 1px solid var(--line); border-radius: 12px; padding: 1rem;
    display: flex; flex-direction: column; gap: 0.55rem;
    background: color-mix(in srgb, var(--ink) 2%, transparent);
  }
  .mcard.bad { opacity: 0.72; }
  .mcard-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 0.5rem; }
  .mcard-head strong { display: block; font-size: 1.02rem; }
  .mcard-head .small { display: block; margin-top: 0.1rem; }
  .caps { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .cap {
    font-size: 0.72rem; color: var(--muted);
    border: 1px solid var(--line); border-radius: 6px; padding: 0.1rem 0.4rem;
  }
  .mblurb { margin: 0; font-size: 0.88rem; line-height: 1.45; color: var(--ink); }
  .mcard-foot { display: flex; align-items: center; gap: 0.8rem; margin-top: auto; }
  .small-btn { width: auto; margin: 0; padding: 0.5rem 0.9rem; font-size: 0.88rem; flex: 1; }

  .tasks { list-style: none; padding: 0; margin: 0; }
  .tasks li {
    display: flex; align-items: center; gap: 0.7rem;
    padding: 0.65rem 0; color: var(--muted);
  }
  .tasks li.active, .tasks li.done { color: var(--ink); }
  .tasks li.error { color: var(--bad); }
  .tick {
    width: 22px; height: 22px; flex: none; border-radius: 50%;
    display: grid; place-items: center; font-size: 0.8rem; font-weight: 700;
    border: 1px solid var(--line);
  }
  .tasks li.done .tick { background: var(--ok); color: #fff; border-color: var(--ok); }
  .tasks li.error .tick { background: var(--bad); color: #fff; border-color: var(--bad); }
  .tasks li.active .tick { border-color: var(--accent); color: var(--accent); }

  .log {
    margin-top: 1rem; max-height: 160px; overflow: auto;
    background: color-mix(in srgb, var(--ink) 6%, transparent);
    border-radius: 8px; padding: 0.7rem; font-size: 0.78rem; line-height: 1.4;
    white-space: pre-wrap; word-break: break-word; color: var(--muted);
  }

  .success { text-align: center; }
  .check {
    width: 56px; height: 56px; margin: 0 auto 0.6rem; border-radius: 50%;
    background: var(--ok); color: #fff; font-size: 1.8rem;
    display: grid; place-items: center;
  }

  /* ---- Modal ---- */
  .scrim {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.45);
    display: grid; place-items: center; padding: 1.5rem; z-index: 10;
  }
  .modal {
    width: 100%; max-width: 440px; background: var(--card);
    border: 1px solid var(--line); border-radius: 16px; padding: 1.6rem;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  }
  .modal h2 { color: var(--bad); }
  .ack { margin: 0.8rem 0 0; padding-left: 1.1rem; }
  .ack li { margin: 0.4rem 0; line-height: 1.45; font-size: 0.92rem; }

  footer { margin-top: auto; padding-top: 1.5rem; color: var(--muted); font-size: 0.8rem; }
</style>
