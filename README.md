# Cairn

**A guided desktop app for running a local LLM — privately, on your own machine.**

Cairn sits between GPT4All and Jan.ai in simplicity, with a power-user "engine
room" underneath. It detects your hardware, installs and manages a local model
engine, and gives you a private chat app — with nothing leaving your computer.

> **Status:** Phase 3. Simple mode + Explore catalog + Remote access. No Docker —
> Cairn installs the engines it manages. macOS + Linux.

## Mission

Cairn exists to put AI back in people's own hands.

Today, using a capable AI model almost always means sending your words to someone
else's datacenter — metered, logged, and outside your control. Yet open models have
become good enough, and ordinary hardware capable enough, to run genuinely useful AI
on the computer already sitting on your desk. The barrier is no longer capability —
it's friction. Running a local model still demands a terminal, Docker know-how, and a
dozen decisions most people shouldn't have to make.

Cairn's job is to erase that friction: to make running your own private AI as
approachable as installing any other app, so that privacy and independence aren't a
luxury reserved for the technically fluent. We believe **local-first AI should be a
default option, not the expert's workaround** — and that democratizing access to it
opens the door to uses that centralized, cloud-metered AI will never serve well: the
private, the offline, the personal, the underfunded, and the experimental.

Not everything has to run in a datacenter. Cairn is a small step toward making sure it
doesn't have to.

## How it works

Cairn is a thin [Tauri](https://tauri.app) shell that orchestrates two engines it
does **not** bundle:

```
Cairn (Tauri + Svelte)
  ├── manages → Ollama   (native, localhost:11434, Metal / CUDA / ROCm — full GPU)
  └── manages → Open WebUI (native, run via uv, auto-picked free port)
                   └── connects to Ollama via 127.0.0.1:11434
```

Both engines run **natively** — no Docker, no VM. Ollama runs natively so it can
reach the GPU (Docker can't access Metal on Apple Silicon), and Open WebUI is run
with [uv](https://docs.astral.sh/uv/), which Cairn installs for you if it's missing.
Nothing to set up by hand.

## What it does

**Simple mode** — a guided, one-path setup:

1. **Detects your hardware** — RAM, GPU/VRAM budget (Apple Silicon unified memory,
   NVIDIA CUDA, AMD ROCm, or CPU-only), free disk, and whether the engines are present.
2. **Recommends one model** for your hardware tier, with a 🟢/🟡/🔴 "will it run?" rating.
3. **Sets everything up** — installs Ollama if needed, downloads the model, and
   launches Open WebUI on a free port.
4. **Opens your assistant** in the browser.

**Explore mode** — a browsable catalog (Phase 2):

- A curated set of models — general chat, reasoning/conversation (Qwen3, Hivemind),
  coding (Qwen2.5-Coder), vision (Qwen2.5-VL, Llama 3.2 Vision), and medical (MedGemma).
- **Each model rated 🟢/🟡/🔴 for your specific machine** and sorted best-fit-first.
- Use-case filter chips, capability tags, and a link out to model benchmarks.
- One-click install of any model into the same chat app. Models with special terms
  (e.g. MedGemma) require an on-screen acknowledgment first.

**Remote access** — reach your AI from other devices (Phase 3):

- **Private by default** — the chat app is published only on `127.0.0.1`, so nothing
  is reachable off-machine unless you opt in.
- Three explained **binding tiers**: 🔒 Private (this computer), 🏠 Local network
  (devices on your Wi-Fi), 🌐 Tailscale (your devices anywhere, encrypted).
- Switching tier restarts the chat app on the new address; the active address is shown
  with a **QR code** to scan from a companion/remote client. Tailscale is
  detect-and-guide (Cairn reads `tailscale status` but never logs you in).

**Uninstall** — a one-button cleanup (footer → Uninstall) stops the chat app, removes
it and its files, and deletes your local chats/accounts/settings. Your Ollama models
are kept. (Dragging the app to the Trash alone won't remove the managed pieces.)

## Requirements

- macOS (Apple Silicon or Intel) or Linux
- Internet connection for the initial setup (Cairn installs the engines on first run,
  then runs offline afterward)

## Install

Prebuilt installers are attached to each
[release](https://github.com/sparkly-quasar/cairn/releases):

- **macOS** — download the `.dmg`, open it, and drag Cairn to Applications. These
  builds aren't notarized yet, so on first launch **right-click the app and choose
  Open** (or run `xattr -dr com.apple.quarantine /Applications/Cairn.app`) to get past
  Gatekeeper. It opens normally after that.
- **Linux** — download the `.AppImage` (make it executable and run it), or the `.deb`
  / `.rpm` for your distribution.

On first launch Cairn installs the engines it needs (Ollama and, via uv, Open WebUI),
so you just need an internet connection for that initial setup.

**Updates are automatic.** Cairn checks on launch and, when a new version is out,
offers to install it in place — one click on **"Install & restart"**, signed and
verified, fully in-app. There's nothing to re-download.

Your models and chat history are never touched: Cairn only *manages* Ollama and Open
WebUI, and your data lives in those, not in the Cairn app.

*(One-time exception: builds from before the updater existed — v0.3.1 or earlier —
need a single manual update to the latest release; auto-updates take over after that.)*

## Development

```bash
# Prerequisites: Rust (rustup) and Node.js
npm install
npm run tauri dev      # launch the app in dev mode
npm run tauri build    # produce a .dmg / AppImage / .deb / .rpm
```

Project layout:

- `src-tauri/src/spec/` — hardware detection (`SystemProfile`)
- `src-tauri/src/engine/` — Ollama + Open WebUI lifecycle (native, run via uv; per-tier host binding)
- `src-tauri/src/server.rs` — remote-access binding tiers, Tailscale/LAN discovery, QR
- `src-tauri/src/rating.rs` — shared "will it run?" 🟢/🟡/🔴 rating logic
- `src-tauri/src/recommend.rs` — hardware-tier single-pick (Simple mode)
- `src-tauri/src/catalog.rs` + `catalog.json` — the Explore-mode model catalog
- `src-tauri/src/commands.rs` — the Tauri command surface
- `src/routes/+page.svelte` — the Simple flow + Explore catalog UI

## Roadmap

- **Phase 2** ✅ — model catalog + capability cards, use-case bundles, MedGemma (with
  medical-use disclaimers).
- **Phase 3** ✅ — server mode (Private / LAN / Tailscale) for remote access from the
  Conduit app, with per-tier binding and QR pairing.
- **Docker dependency dropped** ✅ — Open WebUI now runs natively via uv (installed
  automatically), plus a one-button uninstall. App self-update via Tauri's updater.
- **Phase 4** — advanced mode (quantization, context length, logs).

## License

Licensed under the **[PolyForm Noncommercial License 1.0.0](./LICENSE)**.

You may use, modify, and share Cairn freely for **non-commercial** purposes
(personal use, research, education, nonprofits, government). **Commercial use
requires a separate commercial license — a contract with the author.** For
commercial licensing, contact the author via
[github.com/sparkly-quasar](https://github.com/sparkly-quasar).

See [`NOTICE`](./NOTICE) for third-party components. Note that earlier releases
(`v0.1.0`–`v0.3.1`) were published under Apache-2.0 and remain available under
those terms; this license applies from the next release onward. Model weights are
governed by their own licenses.
