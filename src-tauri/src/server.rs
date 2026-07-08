// SPDX-License-Identifier: Apache-2.0
//! Phase-3 "server mode": expose the local Open WebUI to other devices under an
//! explicit, explained binding tier. The default is **Private** (localhost only)
//! so nothing is reachable off-machine unless the user deliberately opts in.
//!
//! - Private   → container publishes on 127.0.0.1 (this computer only)
//! - Lan       → publishes on 0.0.0.0, reachable at the machine's LAN IP
//! - Tailscale → publishes on 0.0.0.0, reached via the tailnet (MagicDNS) name
//!
//! Tailscale is detect-and-guide only: we read `tailscale status` but never run
//! `tailscale up` ourselves.

use serde::{Deserialize, Serialize};
use std::net::UdpSocket;

/// Which network interface Open WebUI is published on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindTier {
    Private,
    Lan,
    Tailscale,
}

impl BindTier {
    /// Host address Docker should publish the container port on.
    pub fn bind_host(self) -> &'static str {
        match self {
            BindTier::Private => "127.0.0.1",
            BindTier::Lan | BindTier::Tailscale => "0.0.0.0",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            BindTier::Private => "private",
            BindTier::Lan => "lan",
            BindTier::Tailscale => "tailscale",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "private" => Some(BindTier::Private),
            "lan" => Some(BindTier::Lan),
            "tailscale" => Some(BindTier::Tailscale),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TailscaleInfo {
    pub installed: bool,
    pub running: bool,
    pub ipv4: Option<String>,
    /// MagicDNS name (trailing dot stripped), e.g. "isaac.tailnet-1234.ts.net".
    pub dns_name: Option<String>,
}

/// Full picture the Remote-access UI renders.
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub tier: BindTier,
    pub running: bool,
    pub port: Option<u16>,
    pub private_url: Option<String>,
    pub lan_ip: Option<String>,
    pub lan_url: Option<String>,
    pub tailscale: TailscaleInfo,
    pub tailscale_url: Option<String>,
}

// ---- tier persistence (a single line in the app config dir) ----

fn tier_file() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".cairn").join("server-tier"))
}

/// The tier the user last chose; defaults to Private (safest) on first run.
pub fn saved_tier() -> BindTier {
    tier_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| BindTier::parse(&s))
        .unwrap_or(BindTier::Private)
}

pub fn save_tier(tier: BindTier) {
    if let Some(path) = tier_file() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, tier.as_str());
    }
}

// ---- network discovery ----

/// Primary outbound LAN IPv4. Uses the connect-a-UDP-socket trick: no packets
/// are actually sent, but the OS picks the default-route interface's address.
pub fn local_ipv4() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let ip = sock.local_addr().ok()?.ip();
    // Ignore a Tailscale/CG-NAT address if that happened to be chosen.
    let s = ip.to_string();
    if s.starts_with("100.") {
        None
    } else {
        Some(s)
    }
}

pub fn tailscale_info() -> TailscaleInfo {
    let mut info = TailscaleInfo { installed: false, running: false, ipv4: None, dns_name: None };

    let Some(json) = crate::util::run("tailscale", &["status", "--json"]) else {
        return info;
    };
    info.installed = true;

    let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
        return info;
    };
    info.running = v.get("BackendState").and_then(|b| b.as_str()) == Some("Running");

    if let Some(this) = v.get("Self") {
        info.dns_name = this
            .get("DNSName")
            .and_then(|d| d.as_str())
            .map(|d| d.trim_end_matches('.').to_string())
            .filter(|d| !d.is_empty());
        info.ipv4 = this
            .get("TailscaleIPs")
            .and_then(|ips| ips.as_array())
            .and_then(|arr| arr.iter().filter_map(|i| i.as_str()).find(|i| i.contains('.')))
            .map(|s| s.to_string());
    }

    info
}

/// Preferred host for reaching this machine over Tailscale — MagicDNS name if
/// available (works from phones/laptops on the tailnet), else the 100.x IP.
fn tailscale_host(ts: &TailscaleInfo) -> Option<String> {
    ts.dns_name.clone().or_else(|| ts.ipv4.clone())
}

/// Assemble the full status for the given port (None if Open WebUI isn't up).
pub fn status(tier: BindTier, port: Option<u16>) -> ServerStatus {
    let ts = tailscale_info();
    let lan_ip = local_ipv4();

    let (private_url, lan_url, tailscale_url) = match port {
        Some(p) => (
            Some(format!("http://localhost:{p}")),
            lan_ip.as_ref().map(|ip| format!("http://{ip}:{p}")),
            tailscale_host(&ts).map(|h| format!("http://{h}:{p}")),
        ),
        None => (None, None, None),
    };

    ServerStatus {
        tier,
        running: port.is_some(),
        port,
        private_url,
        lan_ip,
        lan_url,
        tailscale: ts,
        tailscale_url,
    }
}

/// Generate an SVG QR code encoding `text`, for the Conduit app to scan.
pub fn qr_svg(text: &str) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .build();
    Ok(svg)
}
