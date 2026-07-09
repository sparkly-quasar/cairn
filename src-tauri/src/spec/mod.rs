// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Hardware spec detection. Produces the `SystemProfile` consumed by the
//! recommendation engine and the frontend "Your computer" screen.

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

/// The machine profile surfaced to the UI. All fields are best-effort:
/// a probe that fails degrades gracefully rather than aborting detection.
#[derive(Debug, Clone, Serialize)]
pub struct SystemProfile {
    pub os: String,
    pub arch: String,
    pub ram_gb: f64,
    /// "apple" | "nvidia" | "amd" | "none"
    pub gpu_vendor: String,
    pub gpu_name: Option<String>,
    /// Fast-memory budget for models: unified-memory budget on Apple Silicon,
    /// real VRAM on discrete GPUs, 0 when CPU-only.
    pub vram_gb: f64,
    pub cpu_cores: u32,
    pub free_disk_gb: f64,
    /// Whether the runtime that powers the chat app (uv) is already installed.
    /// Informational only — Cairn installs it during setup if missing.
    pub uv_present: bool,
    pub ollama_present: bool,
    /// True when GPU support is unproven (e.g. AMD without a working ROCm stack).
    pub gpu_experimental: bool,
}

/// Internal GPU probe result, shared with the per-OS submodules.
pub(crate) struct GpuInfo {
    pub vendor: String,
    pub name: Option<String>,
    pub vram_gb: f64,
    pub experimental: bool,
}

pub fn detect() -> SystemProfile {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let ram_gb = round1(detect_ram_gb());
    let free_disk_gb = round1(detect_free_disk_gb());
    let gpu = detect_gpu(ram_gb);

    SystemProfile {
        os,
        arch,
        ram_gb,
        gpu_vendor: gpu.vendor,
        gpu_name: gpu.name,
        vram_gb: round1(gpu.vram_gb),
        cpu_cores,
        free_disk_gb,
        uv_present: crate::engine::openwebui_native::uv_present(),
        ollama_present: crate::engine::ollama::is_present(),
        gpu_experimental: gpu.experimental,
    }
}

fn detect_gpu(ram_gb: f64) -> GpuInfo {
    #[cfg(target_os = "macos")]
    {
        macos::detect_gpu(ram_gb)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = ram_gb;
        linux::detect_gpu()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = ram_gb;
        GpuInfo {
            vendor: "none".into(),
            name: None,
            vram_gb: 0.0,
            experimental: false,
        }
    }
}

fn detect_ram_gb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        crate::util::run("sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|bytes| bytes as f64 / 1e9)
            .unwrap_or(0.0)
    }
    #[cfg(target_os = "linux")]
    {
        // /proc/meminfo reports MemTotal in kB.
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|c| {
                c.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| {
                    l.split_whitespace().nth(1).and_then(|v| v.parse::<f64>().ok())
                })
            })
            .map(|kb| kb * 1024.0 / 1e9)
            .unwrap_or(0.0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0.0
    }
}

/// Free space on the volume backing $HOME, via `df -k` (available column, index 3).
/// Works identically on macOS and Linux.
fn detect_free_disk_gb() -> f64 {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    crate::util::run("df", &["-k", &home])
        .and_then(|out| {
            out.lines().last().and_then(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                fields.get(3).and_then(|v| v.parse::<f64>().ok())
            })
        })
        .map(|kb| kb * 1024.0 / 1e9)
        .unwrap_or(0.0)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
