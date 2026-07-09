//! Small process helpers shared across modules.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Emitter};

/// Build a `Command` with an enriched `PATH`. GUI apps launched from Finder or
/// the Dock inherit only launchd's bare `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`),
/// so tools installed by Homebrew (`/opt/homebrew/bin`) or `uv`/pipx
/// (`~/.local/bin`) — like `ollama`, `brew`, and `uv` — are invisible and every
/// spawn fails. Prepending the usual install dirs makes them resolvable whether
/// the app runs from a terminal (dev) or the Dock (bundled).
pub fn command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.env("PATH", enriched_path());
    c
}

/// The inherited `PATH` with common Homebrew/user-local bin dirs prepended.
/// Non-existent dirs are harmless; existing entries are preserved after ours.
fn enriched_path() -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
    }
    for d in [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// Run a command with args. Returns trimmed stdout on exit-0, else `None`.
/// A missing binary or non-zero exit is a graceful `None`, never a panic.
pub fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let output = command(cmd).args(args).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Spawn a child, stream its stdout+stderr line-by-line to `event`, and wait.
/// Used for long installs/downloads so the UI can show live progress.
pub fn run_streamed(
    app: &AppHandle,
    cmd: &str,
    args: &[&str],
    event: &'static str,
) -> Result<(), String> {
    let mut child: Child = command(cmd)
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
