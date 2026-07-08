// SPDX-License-Identifier: Apache-2.0
//! Apple Silicon / Intel Mac GPU detection.

use super::GpuInfo;

/// On Apple Silicon the GPU shares unified memory with the CPU. We treat ~70%
/// of total RAM as the practical model budget (headroom for the OS + apps).
const UNIFIED_BUDGET_FRACTION: f64 = 0.70;

pub fn detect_gpu(ram_gb: f64) -> GpuInfo {
    if std::env::consts::ARCH == "aarch64" {
        GpuInfo {
            vendor: "apple".into(),
            name: chip_name(),
            vram_gb: ram_gb * UNIFIED_BUDGET_FRACTION,
            experimental: false,
        }
    } else {
        // Intel Macs: no unified-memory acceleration story worth budgeting for.
        GpuInfo {
            vendor: "none".into(),
            name: chip_name(),
            vram_gb: 0.0,
            experimental: false,
        }
    }
}

/// Parse the "Chip:" (Apple Silicon) or "Processor Name:" (Intel) line from
/// `system_profiler SPHardwareDataType`. Display-only.
fn chip_name() -> Option<String> {
    let out = crate::util::run("system_profiler", &["SPHardwareDataType"])?;
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("Chip:") {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = l.strip_prefix("Processor Name:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}
