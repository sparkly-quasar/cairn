// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Native Ollama: detection, install, and model pulls. Long operations stream
//! their output to the frontend via Tauri events.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;
use tauri::AppHandle;

use crate::util::run_streamed;

const OLLAMA_ADDR: &str = "127.0.0.1:11434";

/// Present if the CLI is installed or the API port is already answering.
pub fn is_present() -> bool {
    crate::util::run("ollama", &["--version"]).is_some() || api_up()
}

/// True if something is listening on the Ollama port.
pub fn api_up() -> bool {
    match OLLAMA_ADDR.parse::<SocketAddr>() {
        Ok(addr) => TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok(),
        Err(_) => false,
    }
}

/// `ollama show <tag>` exits non-zero when the model is not pulled.
pub fn is_model_present(tag: &str) -> bool {
    crate::util::run("ollama", &["show", tag]).is_some()
}

/// Install Ollama. On Linux we use the official script; on macOS we prefer
/// Homebrew, otherwise we point the user at the app download.
pub fn install(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        run_streamed(
            app,
            "sh",
            &["-c", "curl -fsSL https://ollama.com/install.sh | sh"],
            "ollama-install-progress",
        )
    }
    #[cfg(target_os = "macos")]
    {
        if crate::util::run("brew", &["--version"]).is_some() {
            run_streamed(app, "brew", &["install", "ollama"], "ollama-install-progress")?;
            // Start the background service so the API comes up.
            let _ = crate::util::run("brew", &["services", "start", "ollama"]);
            Ok(())
        } else {
            Err("Ollama isn't installed and Homebrew wasn't found. Please install \
                 Ollama from https://ollama.com/download, then click Retry."
                .into())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = app;
        Err("Automatic Ollama install is only supported on macOS and Linux.".into())
    }
}

/// `ollama pull <tag>`, streaming progress to the `pull-progress` event.
pub fn pull(app: &AppHandle, tag: &str) -> Result<(), String> {
    run_streamed(app, "ollama", &["pull", tag], "pull-progress")
        .map_err(|_| format!("Failed to download {tag}. Check your connection and try again."))
}
