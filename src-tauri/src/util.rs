//! Small process helpers shared across modules.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Emitter};

/// Run a command with args. Returns trimmed stdout on exit-0, else `None`.
/// A missing binary or non-zero exit is a graceful `None`, never a panic.
pub fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
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
