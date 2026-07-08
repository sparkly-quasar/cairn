// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Shared "will it run on this machine?" rating logic, used by both the
//! single-pick Simple-mode recommendation and the full Explore catalog.
//!
//! A model is rated green / yellow / red against the machine's effective
//! fast-memory budget (unified/VRAM budget when a GPU is present, else RAM).

use crate::spec::SystemProfile;

/// Effective fast-memory budget in GB.
pub fn budget(p: &SystemProfile) -> f64 {
    if p.gpu_vendor != "none" {
        p.vram_gb
    } else {
        p.ram_gb
    }
}

pub struct Rated {
    /// "green" | "yellow" | "red"
    pub rating: &'static str,
    pub rating_label: &'static str,
    pub reason: String,
}

/// Rate a model of the given memory/disk requirements against the machine.
pub fn rate(p: &SystemProfile, min_ram_gb: f64, min_vram_gb: f64, disk_gb: f64) -> Rated {
    let has_gpu = p.gpu_vendor != "none";
    let b = budget(p);

    let (rating, rating_label, mut reason): (&'static str, &'static str, String) = if has_gpu {
        let where_ = if p.gpu_vendor == "apple" { "unified memory" } else { "GPU memory" };
        if b >= min_vram_gb {
            (
                "green",
                "Comfortable",
                format!("Fits comfortably in your ~{b:.0} GB {where_} budget and runs fast."),
            )
        } else if b >= min_vram_gb * 0.7 {
            (
                "yellow",
                "Will run (slower)",
                "Slightly over your fast-memory budget — it will run with partial \
                 offload and be somewhat slower."
                    .to_string(),
            )
        } else {
            (
                "red",
                "Too big for this machine",
                format!(
                    "Needs about {min_vram_gb:.0} GB of {where_} but you have ~{b:.0} GB — \
                     it would be very slow or fail to load."
                ),
            )
        }
    } else {
        // CPU-only: everything runs slower; gate on system RAM.
        if p.ram_gb >= min_ram_gb {
            (
                "yellow",
                "Runs on CPU (slower)",
                "No dedicated GPU detected — this model runs on your CPU and will \
                 respond more slowly, but it works."
                    .to_string(),
            )
        } else {
            (
                "red",
                "Not enough memory",
                format!(
                    "Needs about {min_ram_gb:.0} GB of memory but you have ~{:.0} GB.",
                    p.ram_gb
                ),
            )
        }
    };

    // Disk headroom caveat (does not change the compute rating).
    if p.free_disk_gb > 0.0 && p.free_disk_gb < disk_gb {
        reason.push_str(&format!(
            " Note: it needs about {disk_gb:.0} GB of disk and you have {:.0} GB free.",
            p.free_disk_gb
        ));
    }

    Rated { rating, rating_label, reason }
}
