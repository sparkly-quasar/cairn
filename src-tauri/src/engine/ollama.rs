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
/// Homebrew; on Windows we use winget — each falling back to pointing the
/// user at the app download.
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
    #[cfg(target_os = "windows")]
    {
        if crate::util::run("winget", &["--version"]).is_some() {
            run_streamed(
                app,
                "winget",
                &[
                    "install",
                    "--id",
                    "Ollama.Ollama",
                    "--silent",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ],
                "ollama-install-progress",
            )?;
            start_windows_service();
            Ok(())
        } else {
            Err("Ollama isn't installed and winget wasn't found. Please install \
                 Ollama from https://ollama.com/download, then click Retry."
                .into())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        Err("Automatic Ollama install is only supported on macOS, Windows, and Linux.".into())
    }
}

/// A silent winget install doesn't launch Ollama the way the interactive
/// installer does, so bring the API up ourselves: start the tray app if we can
/// find it (it also registers the run-at-login task), else a detached
/// `ollama serve`. Then wait briefly for the port so the caller's next step
/// (pulling a model) doesn't race the startup.
#[cfg(target_os = "windows")]
fn start_windows_service() {
    use std::time::Instant;

    if !api_up() {
        let tray_app = std::env::var_os("LOCALAPPDATA").map(|l| {
            std::path::PathBuf::from(l)
                .join("Programs")
                .join("Ollama")
                .join("ollama app.exe")
        });
        let started = match tray_app.filter(|p| p.exists()) {
            Some(p) => crate::util::command(&p.to_string_lossy()).spawn().is_ok(),
            None => false,
        };
        if !started {
            let _ = crate::util::command("ollama")
                .arg("serve")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !api_up() {
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// `ollama pull <tag>`, streaming progress to the `pull-progress` event.
pub fn pull(app: &AppHandle, tag: &str) -> Result<(), String> {
    run_streamed(app, "ollama", &["pull", tag], "pull-progress")
        .map_err(|_| format!("Failed to download {tag}. Check your connection and try again."))
}
