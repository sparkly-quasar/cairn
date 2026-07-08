// SPDX-License-Identifier: Apache-2.0
//! Open WebUI lifecycle via Docker. Connects to the *native* Ollama on the host
//! through `host.docker.internal:11434`.
//!
//! Port note: we do NOT hardcode 3000 — it is frequently occupied (e.g. an
//! existing Open WebUI). We auto-pick the first free port at/above 3000 and
//! name our container `cairn-openwebui` so we never collide with other stacks.

use std::net::TcpListener;
use std::time::{Duration, Instant};

const CONTAINER: &str = "cairn-openwebui";
const IMAGE: &str = "ghcr.io/open-webui/open-webui:main";
const VOLUME: &str = "cairn-openwebui";
const INTERNAL_PORT: &str = "8080";

/// Ensure the Open WebUI container is running and return its base URL.
/// Reuses an existing `cairn-openwebui` container (and its port) if present.
pub fn ensure() -> Result<String, String> {
    if !crate::engine::docker_running() {
        return Err("Docker isn't running. Open Docker Desktop, wait for it to \
                    finish starting, then click Retry."
            .into());
    }

    if container_exists() {
        let _ = crate::util::run("docker", &["start", CONTAINER]);
        if let Some(port) = container_port() {
            wait_until_ready(port);
            return Ok(url(port));
        }
        // Port unreadable (unexpected) — recreate cleanly.
        let _ = crate::util::run("docker", &["rm", "-f", CONTAINER]);
    }

    let port = pick_free_port(3000);
    let port_map = format!("{port}:{INTERNAL_PORT}");
    let volume_map = format!("{VOLUME}:/app/backend/data");
    let args = [
        "run",
        "-d",
        "--name",
        CONTAINER,
        "-p",
        &port_map,
        "-e",
        "OLLAMA_BASE_URL=http://host.docker.internal:11434",
        "--add-host",
        "host.docker.internal:host-gateway",
        "-v",
        &volume_map,
        "--restart",
        "unless-stopped",
        IMAGE,
    ];

    crate::util::run("docker", &args)
        .ok_or_else(|| "Failed to start the Open WebUI container.".to_string())?;

    wait_until_ready(port);
    Ok(url(port))
}

/// Base URL string for a given host port.
fn url(port: u16) -> String {
    format!("http://localhost:{port}")
}

/// First bindable port at/above `preferred`, so we skip anything in use.
fn pick_free_port(preferred: u16) -> u16 {
    for port in preferred..preferred.saturating_add(100) {
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
    // Last resort: let the OS assign one.
    TcpListener::bind(("0.0.0.0", 0))
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(preferred)
}

fn container_exists() -> bool {
    crate::util::run(
        "docker",
        &["ps", "-a", "--filter", &format!("name=^/{CONTAINER}$"), "--format", "{{.Names}}"],
    )
    .map(|s| s.lines().any(|l| l.trim() == CONTAINER))
    .unwrap_or(false)
}

/// Host port mapped to the container's internal port, e.g. from
/// `docker port cairn-openwebui 8080` → "0.0.0.0:3001".
fn container_port() -> Option<u16> {
    let out = crate::util::run("docker", &["port", CONTAINER, INTERNAL_PORT])?;
    out.lines()
        .next()
        .and_then(|l| l.rsplit(':').next())
        .and_then(|p| p.trim().parse().ok())
}

/// Poll the published port until the web server answers, up to ~40s. Open WebUI
/// takes a while on first boot; we return regardless so the UI can proceed and
/// the browser will finish loading once it's up.
fn wait_until_ready(port: u16) {
    let addr = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if let Ok(sa) = addr.parse::<std::net::SocketAddr>() {
            if std::net::TcpStream::connect_timeout(&sa, Duration::from_millis(500)).is_ok() {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(750));
    }
}
