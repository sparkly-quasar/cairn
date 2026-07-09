// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Open WebUI run *natively* via `uv` — no Docker, no VM. Connects straight to
//! the native Ollama on 127.0.0.1:11434.
//!
//! Lifecycle: we install Open WebUI once as a uv tool (streamed, so first-run
//! download shows progress), then spawn `uv tool run open-webui serve` as a
//! detached child. We record its PID, port, and bind host under `~/.cairn` and
//! re-adopt it on the next launch by probing the port — mirroring the "stays up
//! after you close the app" behaviour the Docker `--restart` flag used to give.
//!
//! Data (chats, accounts, settings) lives in `DATA_DIR` so it persists across
//! restarts and version bumps. We deliberately do NOT set `WEBUI_AUTH=false`:
//! the Remote-access tiers rely on Open WebUI's own login, where the first
//! account registered becomes the admin.
//!
//! Binding: `serve --host` takes a single address. Private → 127.0.0.1 (this
//! computer only); Lan/Tailscale → 0.0.0.0. Switching tiers restarts the server
//! on the new host, reusing the same port.

use crate::server::{self, BindTier, ServerStatus};
use crate::util::run_streamed;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// Package spec run via uv. `@latest` mirrors the old Docker image's `:main`
/// tag (cairn tracked upstream head). Pin to e.g. `open-webui==0.10.2` here for
/// byte-for-byte reproducibility across machines.
const OPENWEBUI_PKG: &str = "open-webui";
/// CPython that uv provisions for Open WebUI (downloaded + managed by uv if the
/// host has no matching interpreter — so users need no system Python).
const PY_VERSION: &str = "3.11";
const OLLAMA_URL: &str = "http://127.0.0.1:11434";
/// Streamed to the frontend during uv / Open WebUI provisioning.
pub const PROGRESS_EVENT: &str = "chat-engine-progress";

// ---- paths & persisted state (all under ~/.cairn, alongside server-tier) ----

fn cairn_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cairn")
}
fn data_dir() -> PathBuf {
    cairn_dir().join("open-webui")
}
fn pid_file() -> PathBuf {
    cairn_dir().join("openwebui.pid")
}
fn port_file() -> PathBuf {
    cairn_dir().join("openwebui.port")
}
fn bind_file() -> PathBuf {
    cairn_dir().join("openwebui.bind")
}
fn log_file() -> PathBuf {
    cairn_dir().join("openwebui.log")
}

fn write_state(pid: u32, port: u16, host: &str) {
    let _ = fs::create_dir_all(cairn_dir());
    let _ = fs::write(pid_file(), pid.to_string());
    let _ = fs::write(port_file(), port.to_string());
    let _ = fs::write(bind_file(), host);
}
fn clear_state() {
    let _ = fs::remove_file(pid_file());
    let _ = fs::remove_file(port_file());
    let _ = fs::remove_file(bind_file());
}
fn saved_port() -> Option<u16> {
    fs::read_to_string(port_file()).ok()?.trim().parse().ok()
}
fn launched_host() -> Option<String> {
    fs::read_to_string(bind_file()).ok().map(|s| s.trim().to_string())
}

// ---- uv discovery / provisioning ----

/// Locate the `uv` binary: PATH first, then the standard standalone-installer
/// location (`~/.local/bin`), then common package-manager paths.
fn uv_bin() -> Option<String> {
    if crate::util::run("uv", &["--version"]).is_some() {
        return Some("uv".into());
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    [
        home.join(".local/bin/uv"),
        home.join(".cargo/bin/uv"),
        PathBuf::from("/opt/homebrew/bin/uv"),
        PathBuf::from("/usr/local/bin/uv"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .map(|p| p.to_string_lossy().into_owned())
}

/// Whether the runtime that powers the chat app (uv) is ready. Surfaced on the
/// "Your computer" screen — but never a hard gate, since we can install it.
pub fn uv_present() -> bool {
    uv_bin().is_some()
}

/// Ensure `uv` is available, installing it via the official standalone script
/// if missing (a single static binary → `~/.local/bin`, no admin needed).
fn ensure_uv(app: &AppHandle) -> Result<String, String> {
    if let Some(bin) = uv_bin() {
        return Ok(bin);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        run_streamed(
            app,
            "sh",
            &["-c", "curl -LsSf https://astral.sh/uv/install.sh | sh"],
            PROGRESS_EVENT,
        )?;
        uv_bin().ok_or_else(|| "Installed uv but couldn't locate it afterward.".to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = app;
        Err("Automatic chat-app setup is only supported on macOS and Linux.".into())
    }
}

/// True once Open WebUI has been installed as a uv tool.
fn openwebui_installed(uv: &str) -> bool {
    crate::util::run(uv, &["tool", "list"])
        .map(|s| s.lines().any(|l| l.trim_start().starts_with(OPENWEBUI_PKG)))
        .unwrap_or(false)
}

/// Install Open WebUI as a uv tool on first run (streamed — this is the big
/// download). No-op on later runs, so subsequent launches are fast.
fn ensure_installed(app: &AppHandle, uv: &str) -> Result<(), String> {
    if openwebui_installed(uv) {
        return Ok(());
    }
    run_streamed(
        app,
        uv,
        &["tool", "install", "--python", PY_VERSION, OPENWEBUI_PKG],
        PROGRESS_EVENT,
    )
    .map_err(|_| {
        "Failed to install the chat app (Open WebUI). Check your connection and try again."
            .to_string()
    })
}

// ---- public API (mirrors engine::openwebui so callers can swap engines) ----

/// Ensure Open WebUI is installed and running under the saved binding tier
/// (Private by default), returning its base URL.
pub fn ensure(app: &AppHandle) -> Result<String, String> {
    ensure_with(app, server::saved_tier())
}

/// Ensure the server is running and bound to match `tier`, restarting it on the
/// new host if the current binding differs. Data persists across restarts.
pub fn ensure_with(app: &AppHandle, tier: BindTier) -> Result<String, String> {
    let uv = ensure_uv(app)?;
    ensure_installed(app, &uv)?;

    if let Some(port) = running_port() {
        if launched_host().as_deref() == Some(tier.bind_host()) {
            return Ok(url(port));
        }
        // Tier changed → restart on the same port with the new bind host.
        stop();
        return spawn(&uv, port, tier);
    }
    spawn(&uv, pick_free_port(3000), tier)
}

/// Switch tier, restart to match, and return the resulting status.
pub fn set_tier(app: &AppHandle, tier: BindTier) -> Result<ServerStatus, String> {
    server::save_tier(tier);
    ensure_with(app, tier)?;
    Ok(server::status(tier, running_port()))
}

/// Current server status for the Remote-access UI.
pub fn current_status() -> ServerStatus {
    let tier = server::saved_tier();
    server::status(tier, running_port())
}

// ---- uninstall ----

/// What a one-button uninstall did, for the UI to show afterward.
#[derive(Debug, Clone, Serialize)]
pub struct UninstallReport {
    pub removed: Vec<String>,
    pub note: String,
}

/// Remove Cairn's own footprint: stop the server, uninstall the Open WebUI uv
/// tool (frees its Python environment), and delete all Cairn data & state
/// (chats, accounts, settings, logs) under `~/.cairn`.
///
/// Deliberately left alone: the Ollama engine and your downloaded models, and
/// the `uv` runtime — these are large and/or shared with other apps, so we
/// don't delete them from under the user. The app bundle itself goes to the
/// Trash the normal way.
pub fn uninstall() -> UninstallReport {
    let mut removed = Vec::new();

    if saved_port().is_some() || running_port().is_some() {
        stop();
        removed.push("Stopped the chat app".into());
    }

    if let Some(uv) = uv_bin() {
        if openwebui_installed(&uv)
            && crate::util::run(&uv, &["tool", "uninstall", OPENWEBUI_PKG]).is_some()
        {
            removed.push("Removed the chat app (Open WebUI) and its Python files".into());
        }
    }

    let dir = cairn_dir();
    if dir.exists() && fs::remove_dir_all(&dir).is_ok() {
        removed.push("Deleted your chats, accounts, and settings".into());
    }

    UninstallReport {
        removed,
        note: "Your AI models and the Ollama engine were left in place — they can \
               be large and may be used by other apps. Remove the Ollama app \
               separately if you'd like that space back. You can now drag Cairn to \
               the Trash."
            .into(),
    }
}

// ---- process management ----

fn spawn(uv: &str, port: u16, tier: BindTier) -> Result<String, String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create the data folder: {e}"))?;

    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file())
        .map_err(|e| format!("Couldn't open the chat-app log: {e}"))?;
    let err_log = log.try_clone().map_err(|e| e.to_string())?;

    let host = tier.bind_host();
    let child = crate::util::command(uv)
        .args([
            "tool",
            "run",
            "--python",
            PY_VERSION,
            OPENWEBUI_PKG,
            "serve",
            "--host",
            host,
            "--port",
            &port.to_string(),
        ])
        // Run from the data dir. Open WebUI writes `.webui_secret_key` relative
        // to the working directory, and a Finder/Dock-launched app inherits
        // CWD `/` (the read-only system volume) — so without this the server
        // crashes on first boot with "Read-only file system: /.webui_secret_key".
        // Anchoring CWD here keeps that (and any other relative paths) writable
        // and persistent alongside the rest of Open WebUI's data.
        .current_dir(&dir)
        .env("DATA_DIR", &dir)
        .env("OLLAMA_BASE_URL", OLLAMA_URL)
        .env("ANONYMIZED_TELEMETRY", "false")
        .env("WEBUI_NAME", "Cairn")
        // Detach: no stdin, output to a log file — so the server keeps running
        // after Cairn is closed and never blocks on a controlling terminal.
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log))
        .spawn()
        .map_err(|e| format!("Couldn't start the chat app: {e}"))?;

    write_state(child.id(), port, host);
    wait_until_ready(port);
    Ok(url(port))
}

/// Stop the running server: kill the recorded PID *and* whatever holds the port
/// (uv may launch the Python server as a grandchild), then wait for release.
fn stop() {
    let port = saved_port();
    if let Ok(pid) = fs::read_to_string(pid_file()) {
        let pid = pid.trim();
        if !pid.is_empty() {
            let _ = crate::util::run("kill", &[pid]);
        }
    }
    if let Some(p) = port {
        kill_port(p);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && port_answers(p) {
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    clear_state();
}

/// Kill every process listening on `port` (SIGTERM, then SIGKILL if it lingers).
fn kill_port(port: u16) {
    let filter = format!("tcp:{port}");
    let listeners = || crate::util::run("lsof", &["-ti", &filter, "-sTCP:LISTEN"]);

    if let Some(out) = listeners() {
        for pid in out.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let _ = crate::util::run("kill", &[pid]);
        }
        std::thread::sleep(Duration::from_millis(400));
        if let Some(out) = listeners() {
            for pid in out.lines().map(str::trim).filter(|s| !s.is_empty()) {
                let _ = crate::util::run("kill", &["-9", pid]);
            }
        }
    }
}

/// The port we're serving on, if the server is actually answering there.
fn running_port() -> Option<u16> {
    let port = saved_port()?;
    port_answers(port).then_some(port)
}

fn port_answers(port: u16) -> bool {
    format!("127.0.0.1:{port}")
        .parse::<SocketAddr>()
        .ok()
        .map(|sa| TcpStream::connect_timeout(&sa, Duration::from_millis(500)).is_ok())
        .unwrap_or(false)
}

fn url(port: u16) -> String {
    // Use 127.0.0.1, not `localhost`: it's the exact interface `wait_until_ready`
    // probes and the server binds for the Private tier, so we never hand the
    // browser a `localhost` that resolves to IPv6 `::1` and refuses.
    format!("http://127.0.0.1:{port}")
}

/// First bindable port at/above `preferred`, so we skip anything in use.
fn pick_free_port(preferred: u16) -> u16 {
    for port in preferred..preferred.saturating_add(100) {
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
    TcpListener::bind(("0.0.0.0", 0))
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(preferred)
}

/// Poll the port until the web server answers. The first-ever boot downloads
/// the RAG embedding model (~90MB) before it starts listening, so we stay
/// patient (~3min) rather than reporting "ready" too early; later boots answer
/// in well under 10s. We return regardless once the deadline passes so the UI
/// can proceed and the browser finishes loading when it's up.
fn wait_until_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if port_answers(port) {
            return;
        }
        std::thread::sleep(Duration::from_millis(750));
    }
}
