# Cairn

**A guided desktop app for running a local LLM — privately, on your own machine.**

Cairn sits between GPT4All and Jan.ai in simplicity, with a power-user "engine
room" underneath. It detects your hardware, installs and manages a local model
engine, and gives you a private chat app — with nothing leaving your computer.

> **Status:** Phase 3. Simple mode + Explore catalog + Remote access. macOS + Linux.

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
  └── manages → Open WebUI (Docker, auto-picked free port)
                   └── connects to Ollama via host.docker.internal:11434
```

Ollama runs **natively** (not in Docker) so it can reach the GPU — Docker can't
access Metal on Apple Silicon. Open WebUI runs in Docker for a reliable chat UI.

## What it does

**Simple mode** — a guided, one-path setup:

1. **Detects your hardware** — RAM, GPU/VRAM budget (Apple Silicon unified memory,
   NVIDIA CUDA, AMD ROCm, or CPU-only), free disk, and whether Docker/Ollama are present.
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
- Switching tier rebinds the container; the active address is shown with a **QR code**
  to scan from a companion/remote client. Tailscale is detect-and-guide (Cairn reads
  `tailscale status` but never logs you in).

## Requirements

- macOS (Apple Silicon or Intel) or Linux
- [Docker Desktop](https://www.docker.com/products/docker-desktop/) (for the chat UI)
- Internet connection for the initial download (runs offline afterward)

## Development

```bash
# Prerequisites: Rust (rustup) and Node.js
npm install
npm run tauri dev      # launch the app in dev mode
npm run tauri build    # produce a .dmg / AppImage / .deb / .rpm
```

Project layout:

- `src-tauri/src/spec/` — hardware detection (`SystemProfile`)
- `src-tauri/src/engine/` — Ollama + Open WebUI lifecycle (per-tier container binding)
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
- **Phase 4** — advanced mode (quantization, context length, logs), and a bundled
  Python sidecar to drop the Docker dependency. App self-update via Tauri's updater.

## License

Licensed under the [Apache License 2.0](./LICENSE). See [`NOTICE`](./NOTICE) for
third-party components. Model weights are governed by their own licenses.
