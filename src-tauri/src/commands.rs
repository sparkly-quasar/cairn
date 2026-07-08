// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Tauri command surface exposed to the Svelte frontend. Long-running work runs
//! on the blocking pool so the UI thread stays responsive; progress is streamed
//! via events (`ollama-install-progress`, `pull-progress`).

use crate::catalog::{self, Bundle, RatedModel};
use crate::engine::{self, ollama, openwebui};
use crate::recommend::{self, Recommendation};
use crate::server::{self, BindTier, ServerStatus};
use crate::spec::{self, SystemProfile};
use tauri::AppHandle;

#[tauri::command]
pub async fn detect_system() -> SystemProfile {
    tauri::async_runtime::spawn_blocking(spec::detect)
        .await
        .expect("spec detection panicked")
}

#[tauri::command]
pub async fn get_recommendation() -> Recommendation {
    tauri::async_runtime::spawn_blocking(|| recommend::recommend(&spec::detect()))
        .await
        .expect("recommendation panicked")
}

#[tauri::command]
pub async fn get_catalog() -> Vec<RatedModel> {
    tauri::async_runtime::spawn_blocking(|| catalog::catalog(&spec::detect()))
        .await
        .expect("catalog rating panicked")
}

#[tauri::command]
pub fn get_bundles() -> Vec<Bundle> {
    catalog::bundles()
}

#[tauri::command]
pub async fn is_model_present(tag: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || ollama::is_model_present(&tag))
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub fn docker_running() -> bool {
    engine::docker_running()
}

#[tauri::command]
pub async fn install_ollama(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || ollama::install(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pull_model(app: AppHandle, tag: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || ollama::pull(&app, &tag))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ensure_openwebui() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(openwebui::ensure)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn server_status() -> ServerStatus {
    tauri::async_runtime::spawn_blocking(openwebui::current_status)
        .await
        .expect("server status panicked")
}

#[tauri::command]
pub async fn set_server_tier(tier: BindTier) -> Result<ServerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || openwebui::set_tier(tier))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn qr_svg(text: String) -> Result<String, String> {
    server::qr_svg(&text)
}

/// Open a URL in the user's default browser (native, outside the app window).
#[tauri::command]
pub fn open_chat(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let spawned = std::process::Command::new("xdg-open").arg(&url).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let spawned: std::io::Result<std::process::Child> =
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported platform"));

    spawned.map(|_| ()).map_err(|e| format!("Could not open browser: {e}"))
}
