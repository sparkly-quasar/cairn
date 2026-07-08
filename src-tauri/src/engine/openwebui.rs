// SPDX-License-Identifier: Apache-2.0
//! Open WebUI lifecycle via Docker. Connects to the *native* Ollama on the host
//! through `host.docker.internal:11434`.
//!
//! Port note: we do NOT hardcode 3000 — it is frequently occupied (e.g. an
//! existing Open WebUI). We auto-pick the first free port at/above 3000 and
//! name our container `cairn-openwebui` so we never collide with other stacks.
//!
//! Binding note (Phase 3): the container publishes on 127.0.0.1 by default
//! (Private tier) so it is not reachable off-machine. The Remote-access UI can
//! switch it to 0.0.0.0 (LAN / Tailscale) deliberately. See `crate::server`.

use crate::server::{self, BindTier, ServerStatus};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const CONTAINER: &str = "cairn-openwebui";
const IMAGE: &str = "ghcr.io/open-webui/open-webui:main";
const VOLUME: &str = "cairn-openwebui";
const INTERNAL_PORT: &str = "8080";

/// Ensure Open WebUI is running under the user's saved binding tier (Private by
/// default) and return its base URL.
pub fn ensure() -> Result<String, String> {
    ensure_with(server::saved_tier())
}

/// Ensure the container is running and bound to match `tier`, recreating it if
/// the current binding differs. Data persists in the named volume across
/// recreation.
pub fn ensure_with(tier: BindTier) -> Result<String, String> {
    if !crate::engine::docker_running() {
        return Err("Docker isn't running. Open Docker Desktop, wait for it to \
                    finish starting, then click Retry."
            .into());
    }

    if container_exists() {
        if let Some(port) = container_port() {
            if bind_matches(tier) {
                let _ = crate::util::run("docker", &["start", CONTAINER]);
                wait_until_ready(port);
                return Ok(url(port));
            }
            // Tier changed → recreate on the same port with the new bind host.
            let _ = crate::util::run("docker", &["rm", "-f", CONTAINER]);
            return create(port, tier);
        }
        // Port unreadable / stopped — recreate cleanly.
        let _ = crate::util::run("docker", &["rm", "-f", CONTAINER]);
    }

    create(pick_free_port(3000), tier)
}

/// Switch the binding tier, recreate the container to match, and return the
/// resulting server status.
pub fn set_tier(tier: BindTier) -> Result<ServerStatus, String> {
    server::save_tier(tier);
    ensure_with(tier)?;
    Ok(server::status(tier, container_port()))
}

/// Current server status for the Remote-access UI.
pub fn current_status() -> ServerStatus {
    let tier = server::saved_tier();
    let port = if container_running() { container_port() } else { None };
    server::status(tier, port)
}

fn create(port: u16, tier: BindTier) -> Result<String, String> {
    let port_map = format!("{}:{}:{}", tier.bind_host(), port, INTERNAL_PORT);
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

fn container_running() -> bool {
    crate::util::run(
        "docker",
        &["ps", "--filter", &format!("name=^/{CONTAINER}$"), "--format", "{{.Names}}"],
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

/// Host address the container currently publishes on ("127.0.0.1" or "0.0.0.0").
fn container_bind_host() -> Option<String> {
    let out = crate::util::run("docker", &["port", CONTAINER, INTERNAL_PORT])?;
    let line = out.lines().next()?;
    let idx = line.rfind(':')?;
    Some(line[..idx].trim().to_string())
}

/// Whether the running container's publish address already matches `tier`.
fn bind_matches(tier: BindTier) -> bool {
    match container_bind_host().as_deref() {
        Some("127.0.0.1") => tier == BindTier::Private,
        Some("0.0.0.0") => tier != BindTier::Private,
        _ => false,
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    fn reachable(host: &str, port: u16) -> bool {
        format!("{host}:{port}")
            .parse::<SocketAddr>()
            .ok()
            .map(|sa| TcpStream::connect_timeout(&sa, Duration::from_millis(800)).is_ok())
            .unwrap_or(false)
    }

    // Live smoke test: needs Docker + the open-webui image. Run explicitly:
    //   cargo test --lib -- --ignored --nocapture tier_binding
    #[test]
    #[ignore]
    fn tier_binding_controls_reachability() {
        if !crate::engine::docker_running() {
            eprintln!("skip: docker not running");
            return;
        }
        let lan = crate::server::local_ipv4().expect("need a LAN ip");
        eprintln!("LAN ip = {lan}");

        // LAN tier → published on 0.0.0.0, reachable via the LAN address.
        let s = set_tier(BindTier::Lan).expect("set lan");
        let port = s.port.expect("port after lan");
        eprintln!("lan tier: bind_host={:?} port={port}", container_bind_host());
        assert_eq!(container_bind_host().as_deref(), Some("0.0.0.0"));
        assert!(reachable("127.0.0.1", port), "localhost must work on LAN tier");
        assert!(reachable(&lan, port), "LAN address must be reachable on LAN tier");

        // Private tier → published on 127.0.0.1 only; LAN address must be refused.
        let s = set_tier(BindTier::Private).expect("set private");
        let port = s.port.expect("port after private");
        eprintln!("private tier: bind_host={:?} port={port}", container_bind_host());
        assert_eq!(container_bind_host().as_deref(), Some("127.0.0.1"));
        assert!(reachable("127.0.0.1", port), "localhost must still work on Private tier");
        assert!(!reachable(&lan, port), "LAN address must be REFUSED on Private tier");

        // cleanup
        let _ = crate::util::run("docker", &["rm", "-f", CONTAINER]);
    }
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
