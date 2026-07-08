// SPDX-License-Identifier: Apache-2.0
//! Native Ollama: detection, install, and model pulls. Long operations stream
//! their output to the frontend via Tauri events.

use std::io::{BufRead, BufReader, Read};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

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

/// Spawn a child, stream its stdout+stderr line-by-line to `event`, and wait.
fn run_streamed(app: &AppHandle, cmd: &str, args: &[&str], event: &'static str) -> Result<(), String> {
    let mut child: Child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start `{cmd}`: {e}"))?;

    if let Some(out) = child.stdout.take() {
        pump(app.clone(), out, event);
    }
    if let Some(err) = child.stderr.take() {
        pump(app.clone(), err, event);
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{cmd}` exited with {status}"))
    }
}

/// Forward a child pipe's lines to the frontend on a background thread.
fn pump<R: Read + Send + 'static>(app: AppHandle, reader: R, event: &'static str) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let _ = app.emit(event, line);
        }
    });
}
