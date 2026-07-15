// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Windows GPU detection: NVIDIA (CUDA) is the best case — `nvidia-smi` ships
//! with the driver and reports real VRAM. Anything else falls back to a WMI
//! query for the adapter name; AMD is flagged experimental (Ollama's ROCm
//! support on Windows covers only some cards), and Intel/other adapters are
//! treated as CPU-only for budgeting.
//!
//! NOTE: this module is only compiled on Windows, so it is exercised on
//! Windows hardware, not on the macOS dev box.

use super::GpuInfo;

pub fn detect_gpu() -> GpuInfo {
    if let Some(gpu) = detect_nvidia() {
        return gpu;
    }
    if let Some(gpu) = detect_via_wmi() {
        return gpu;
    }
    GpuInfo {
        vendor: "none".into(),
        name: None,
        vram_gb: 0.0,
        experimental: false,
    }
}

fn detect_nvidia() -> Option<GpuInfo> {
    let out = crate::util::run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )?;
    let line = out.lines().next()?;
    let parts: Vec<&str> = line.split(',').collect();
    let name = parts.first().map(|s| s.trim().to_string());
    let vram_mb: f64 = parts.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
    Some(GpuInfo {
        vendor: "nvidia".into(),
        name,
        vram_gb: vram_mb / 1024.0,
        experimental: false,
    })
}

/// Name the display adapter via WMI when nvidia-smi isn't available. WMI's
/// `AdapterRAM` is a 32-bit field that caps at 4GB, so we don't trust it for a
/// VRAM budget — we only use the name to pick a vendor.
fn detect_via_wmi() -> Option<GpuInfo> {
    let out = crate::util::run_powershell(
        "(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name) -join \"`n\"",
    )?;
    let names: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let low = out.to_lowercase();

    if low.contains("nvidia") {
        // Driver present but nvidia-smi missing/broken → unproven acceleration.
        return Some(GpuInfo {
            vendor: "nvidia".into(),
            name: pick_name(&names, "nvidia"),
            vram_gb: 0.0,
            experimental: true,
        });
    }
    if low.contains("amd") || low.contains("radeon") {
        return Some(GpuInfo {
            vendor: "amd".into(),
            name: pick_name(&names, "amd").or_else(|| pick_name(&names, "radeon")),
            vram_gb: 0.0,
            experimental: true,
        });
    }
    None
}

fn pick_name(names: &[&str], vendor: &str) -> Option<String> {
    names
        .iter()
        .find(|n| n.to_lowercase().contains(vendor))
        .map(|n| n.to_string())
}
