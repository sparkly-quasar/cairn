// SPDX-License-Identifier: Apache-2.0
//! Linux GPU detection: NVIDIA (CUDA) is the best case, AMD (ROCm) is flagged
//! experimental unless a working rocm-smi reports VRAM.
//!
//! NOTE: this module is only compiled on Linux, so it is exercised on Linux
//! hardware, not on the macOS dev box.

use super::GpuInfo;

pub fn detect_gpu() -> GpuInfo {
    if let Some(gpu) = detect_nvidia() {
        return gpu;
    }
    if let Some(gpu) = detect_amd() {
        return gpu;
    }
    // Fall back to lspci to at least name a GPU vendor we can't accelerate yet.
    if let Some(gpu) = detect_via_lspci() {
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

fn detect_amd() -> Option<GpuInfo> {
    let out = crate::util::run("rocm-smi", &["--showmeminfo", "vram", "--csv"])?;
    let vram_gb = parse_rocm_vram(&out).unwrap_or(0.0);
    Some(GpuInfo {
        vendor: "amd".into(),
        name: Some("AMD GPU (ROCm)".into()),
        vram_gb,
        // If we could not read a VRAM total, treat support as unproven.
        experimental: vram_gb == 0.0,
    })
}

/// Best-effort: rocm-smi CSV columns vary by version, so scan for the largest
/// byte-count field and convert it.
fn parse_rocm_vram(out: &str) -> Option<f64> {
    let mut best: u64 = 0;
    for line in out.lines() {
        for field in line.split(',') {
            if let Ok(bytes) = field.trim().parse::<u64>() {
                if bytes > best {
                    best = bytes;
                }
            }
        }
    }
    if best > 1_000_000_000 {
        Some(best as f64 / 1e9)
    } else {
        None
    }
}

fn detect_via_lspci() -> Option<GpuInfo> {
    let out = crate::util::run("sh", &["-c", "lspci | grep -Ei 'vga|3d|display'"])?;
    let low = out.to_lowercase();
    if low.contains("nvidia") {
        return Some(GpuInfo {
            vendor: "nvidia".into(),
            name: Some("NVIDIA GPU".into()),
            vram_gb: 0.0,
            experimental: true,
        });
    }
    if low.contains("amd") || low.contains("radeon") || low.contains("advanced micro") {
        return Some(GpuInfo {
            vendor: "amd".into(),
            name: Some("AMD GPU".into()),
            vram_gb: 0.0,
            experimental: true,
        });
    }
    None
}
