<!-- SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    detectSystem,
    getRecommendation,
    isModelPresent,
    installOllama,
    pullModel,
    ensureOpenWebui,
    openChat,
    onProgress,
    type SystemProfile,
    type Recommendation,
  } from "$lib/api";

  type Step = "welcome" | "specs" | "recommend" | "setup" | "done";
  type TaskStatus = "pending" | "active" | "done" | "skipped" | "error";

  let step = $state<Step>("welcome");
  let profile = $state<SystemProfile | null>(null);
  let rec = $state<Recommendation | null>(null);
  let loading = $state(false);
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

  async function runSetup() {
    if (!profile || !rec) return;
    step = "setup";
    setupError = null;
    log = [];
    tasks.forEach((t) => setTask(t.key, "pending"));

    try {
      // 1 — Ollama engine
      if (profile.ollama_present) {
        setTask("ollama", "skipped");
      } else {
        setTask("ollama", "active");
        await installOllama();
        setTask("ollama", "done");
      }

      // 2 — Model
      setTask("model", "active");
      const present = await isModelPresent(rec.ollama_tag);
      if (present) {
        pushLog(`${rec.display_name} is already downloaded.`);
      } else {
        await pullModel(rec.ollama_tag);
      }
      setTask("model", "done");

      // 3 — Open WebUI
      setTask("webui", "active");
      chatUrl = await ensureOpenWebui();
      setTask("webui", "done");

      step = "done";
    } catch (err) {
      const active = tasks.find((t) => t.status === "active");
      if (active) setTask(active.key, "error");
      setupError = typeof err === "string" ? err : String(err);
    }
  }

  // ---- display helpers ----
  const vendorLabel: Record<string, string> = {
    apple: "Apple Silicon (unified memory)",
    nvidia: "NVIDIA GPU",
    amd: "AMD GPU",
    none: "No dedicated GPU",
  };
  const gb = (n: number) => `${n.toFixed(n < 10 ? 1 : 0)} GB`;
  const ratingClass = (r?: string) => (r === "green" ? "ok" : r === "yellow" ? "warn" : "bad");
  const statusIcon = (s: TaskStatus) =>
    s === "done" ? "✓" : s === "skipped" ? "–" : s === "error" ? "✕" : s === "active" ? "…" : "";
</script>

<main>
  <header class="brand">
    <div class="cairn" aria-hidden="true">
      <span></span><span></span><span></span>
    </div>
    <h1>Cairn</h1>
    <p class="tagline">Your own AI, running privately on this computer.</p>
  </header>

  {#if step === "welcome"}
    <section class="card">
      <h2>Let's set up your private assistant</h2>
      <p>
        Cairn installs a local AI model and a chat app on your machine. Nothing you type
        leaves your computer — it runs entirely offline once set up. This takes a few minutes.
      </p>
      <button class="primary" onclick={goSpecs}>Get started</button>
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
        <button class="primary" onclick={runSetup}>Set up my assistant</button>
        <button class="ghost" onclick={goSpecs}>Back</button>
      {/if}
    </section>
  {/if}

  {#if step === "setup"}
    <section class="card">
      <h2>Setting things up</h2>
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
      {/if}
    </section>
  {/if}

  {#if step === "done"}
    <section class="card success">
      <div class="check">✓</div>
      <h2>You're all set</h2>
      <p>Your private assistant is running. Click below to start chatting in your browser.</p>
      <button class="primary" onclick={() => chatUrl && openChat(chatUrl)}>Open my assistant</button>
      {#if chatUrl}<p class="muted small">Running at {chatUrl}</p>{/if}
    </section>
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
    max-width: 560px;
    margin: 0 auto;
    padding: 2.4rem 1.5rem 1.5rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    min-height: 100vh;
    box-sizing: border-box;
  }

  .brand { text-align: center; margin-bottom: 1.6rem; }
  .cairn { display: flex; flex-direction: column; align-items: center; gap: 3px; margin-bottom: 0.5rem; }
  .cairn span {
    display: block; background: var(--accent); border-radius: 40%;
  }
  .cairn span:nth-child(1) { width: 20px; height: 12px; }
  .cairn span:nth-child(2) { width: 32px; height: 14px; }
  .cairn span:nth-child(3) { width: 44px; height: 16px; }
  h1 { margin: 0.2rem 0 0; font-size: 1.9rem; letter-spacing: -0.02em; }
  .tagline { margin: 0.3rem 0 0; color: var(--muted); }

  .card {
    width: 100%;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 16px;
    padding: 1.6rem;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.05);
    box-sizing: border-box;
  }
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
    border: 1px solid currentColor;
  }
  .badge.ok { color: var(--ok); }
  .badge.warn { color: var(--warn); }
  .badge.bad { color: var(--bad); }

  .notice { border-radius: 10px; padding: 0.7rem 0.9rem; font-size: 0.92rem; }
  .notice.bad { background: color-mix(in srgb, var(--bad) 12%, transparent); }

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

  footer { margin-top: auto; padding-top: 1.5rem; color: var(--muted); font-size: 0.8rem; }
</style>
