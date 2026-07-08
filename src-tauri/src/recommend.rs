// SPDX-License-Identifier: Apache-2.0
//! Phase-1 recommendation engine: pick ONE model for the detected hardware tier
//! and rate how comfortably it will run. Baseline assumes a Q4 quant.
//!
//! The full catalog + capability cards land in Phase 2; this is the single-pick
//! MVP that drives the Simple-mode flow.

use crate::spec::SystemProfile;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub model_id: String,
    pub display_name: String,
    pub ollama_tag: String,
    pub disk_gb: f64,
    pub min_ram_gb: f64,
    /// "green" | "yellow" | "red"
    pub rating: String,
    pub rating_label: String,
    pub reason: String,
}

struct Tier {
    model_id: &'static str,
    display_name: &'static str,
    ollama_tag: &'static str,
    disk_gb: f64,
    min_ram_gb: f64,
    min_vram_gb: f64,
}

/// Effective fast-memory budget: unified/VRAM budget when a GPU is present,
/// otherwise system RAM.
fn budget(p: &SystemProfile) -> f64 {
    if p.gpu_vendor != "none" {
        p.vram_gb
    } else {
        p.ram_gb
    }
}

fn tier_for(budget_gb: f64) -> Tier {
    if budget_gb <= 8.0 {
        Tier { model_id: "llama3.2-3b", display_name: "Llama 3.2 3B", ollama_tag: "llama3.2:3b", disk_gb: 2.0, min_ram_gb: 8.0, min_vram_gb: 4.0 }
    } else if budget_gb <= 16.0 {
        Tier { model_id: "llama3.1-8b", display_name: "Llama 3.1 8B", ollama_tag: "llama3.1:8b", disk_gb: 4.7, min_ram_gb: 8.0, min_vram_gb: 6.0 }
    } else if budget_gb <= 32.0 {
        Tier { model_id: "qwen2.5-14b", display_name: "Qwen2.5 14B", ollama_tag: "qwen2.5:14b", disk_gb: 9.0, min_ram_gb: 16.0, min_vram_gb: 10.0 }
    } else {
        Tier { model_id: "qwen2.5-32b", display_name: "Qwen2.5 32B", ollama_tag: "qwen2.5:32b", disk_gb: 20.0, min_ram_gb: 32.0, min_vram_gb: 22.0 }
    }
}

pub fn recommend(p: &SystemProfile) -> Recommendation {
    let b = budget(p);
    let tier = tier_for(b);
    let has_gpu = p.gpu_vendor != "none";

    let (rating, rating_label, mut reason) = if !has_gpu {
        (
            "yellow",
            "Will run (slower)",
            "No dedicated GPU detected — this model runs on your CPU and will \
             respond more slowly, but it works."
                .to_string(),
        )
    } else if b >= tier.min_vram_gb {
        let where_ = if p.gpu_vendor == "apple" { "unified memory" } else { "GPU memory" };
        (
            "green",
            "Comfortable",
            format!("Fits comfortably in your ~{:.0} GB {where_} budget and runs fast.", b),
        )
    } else {
        (
            "yellow",
            "Will run (slower)",
            "Slightly over your fast-memory budget — it will run with partial \
             offload and be somewhat slower."
                .to_string(),
        )
    };

    // Disk headroom caveat (does not change the compute rating).
    if p.free_disk_gb > 0.0 && p.free_disk_gb < tier.disk_gb {
        reason.push_str(&format!(
            " Note: it needs about {:.0} GB of disk and you have {:.0} GB free.",
            tier.disk_gb, p.free_disk_gb
        ));
    }

    Recommendation {
        model_id: tier.model_id.to_string(),
        display_name: tier.display_name.to_string(),
        ollama_tag: tier.ollama_tag.to_string(),
        disk_gb: tier.disk_gb,
        min_ram_gb: tier.min_ram_gb,
        rating: rating.to_string(),
        rating_label: rating_label.to_string(),
        reason,
    }
}
