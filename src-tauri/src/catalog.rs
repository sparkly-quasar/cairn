// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Phase-2 model catalog. A curated set of models (embedded at compile time as
//! `catalog.json`) is rated against the detected machine so the Explore UI can
//! show a per-model green / yellow / red badge and group models into use-case
//! bundles.

use crate::rating;
use crate::spec::SystemProfile;
use serde::{Deserialize, Serialize};

/// Static per-model metadata as authored in `catalog.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub display_name: String,
    pub ollama_tag: String,
    pub params: String,
    pub disk_gb: f64,
    pub min_ram_gb: f64,
    pub min_vram_gb: f64,
    pub capabilities: Vec<String>,
    pub use_cases: Vec<String>,
    pub blurb: String,
    pub license: String,
    pub library_url: String,
    /// Present only for models that require an acknowledgment before install.
    pub requires_ack: Option<Ack>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Ack {
    pub headline: String,
    pub points: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Bundle {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub blurb: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    bundles: Vec<Bundle>,
    models: Vec<CatalogModel>,
}

/// A catalog model plus its rating for the current machine — what the frontend
/// actually renders.
#[derive(Debug, Clone, Serialize)]
pub struct RatedModel {
    pub id: String,
    pub display_name: String,
    pub ollama_tag: String,
    pub params: String,
    pub disk_gb: f64,
    pub min_ram_gb: f64,
    pub capabilities: Vec<String>,
    pub use_cases: Vec<String>,
    pub blurb: String,
    pub license: String,
    pub library_url: String,
    pub requires_ack: Option<Ack>,
    /// "green" | "yellow" | "red"
    pub rating: String,
    pub rating_label: String,
    pub reason: String,
}

const CATALOG_JSON: &str = include_str!("catalog.json");

fn load() -> CatalogFile {
    // The catalog is embedded at compile time, so a parse failure is a build-time
    // authoring bug, not a runtime condition — fail loudly.
    serde_json::from_str(CATALOG_JSON).expect("catalog.json is malformed")
}

pub fn bundles() -> Vec<Bundle> {
    load().bundles
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_json_is_valid_and_consistent() {
        let cf = load(); // panics if catalog.json is malformed
        assert!(!cf.models.is_empty(), "catalog has no models");
        assert!(!cf.bundles.is_empty(), "catalog has no bundles");

        let bundle_ids: HashSet<&str> = cf.bundles.iter().map(|b| b.id.as_str()).collect();
        let mut model_ids: HashSet<&str> = HashSet::new();
        let known_caps: HashSet<&str> = [
            "chat", "reasoning", "code", "vision", "medical", "multilingual", "fast",
            "conversational",
        ]
        .into_iter()
        .collect();

        for m in &cf.models {
            assert!(model_ids.insert(m.id.as_str()), "duplicate model id: {}", m.id);
            assert!(!m.ollama_tag.is_empty(), "{} has empty ollama_tag", m.id);
            assert!(m.disk_gb > 0.0, "{} has non-positive disk_gb", m.id);
            assert!(!m.use_cases.is_empty(), "{} lists no use_cases", m.id);
            for uc in &m.use_cases {
                assert!(bundle_ids.contains(uc.as_str()), "{} → unknown bundle '{}'", m.id, uc);
            }
            for cap in &m.capabilities {
                assert!(known_caps.contains(cap.as_str()), "{} → unknown capability '{}'", m.id, cap);
            }
        }
    }
}

/// The full catalog, each model rated against the given machine. Sorted so the
/// most comfortable fits surface first.
pub fn catalog(p: &SystemProfile) -> Vec<RatedModel> {
    let mut rated: Vec<RatedModel> = load()
        .models
        .into_iter()
        .map(|m| {
            let r = rating::rate(p, m.min_ram_gb, m.min_vram_gb, m.disk_gb);
            RatedModel {
                id: m.id,
                display_name: m.display_name,
                ollama_tag: m.ollama_tag,
                params: m.params,
                disk_gb: m.disk_gb,
                min_ram_gb: m.min_ram_gb,
                capabilities: m.capabilities,
                use_cases: m.use_cases,
                blurb: m.blurb,
                license: m.license,
                library_url: m.library_url,
                requires_ack: m.requires_ack,
                rating: r.rating.to_string(),
                rating_label: r.rating_label.to_string(),
                reason: r.reason,
            }
        })
        .collect();

    // green (0) → yellow (1) → red (2); preserve authored order within a tier.
    rated.sort_by_key(|m| match m.rating.as_str() {
        "green" => 0,
        "yellow" => 1,
        _ => 2,
    });
    rated
}
