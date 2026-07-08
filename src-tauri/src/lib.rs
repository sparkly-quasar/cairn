// SPDX-License-Identifier: Apache-2.0

mod catalog;
mod commands;
mod engine;
mod rating;
mod recommend;
mod server;
mod spec;
mod util;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect_system,
            commands::get_recommendation,
            commands::get_catalog,
            commands::get_bundles,
            commands::is_model_present,
            commands::docker_running,
            commands::install_ollama,
            commands::pull_model,
            commands::ensure_openwebui,
            commands::server_status,
            commands::set_server_tier,
            commands::qr_svg,
            commands::open_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
