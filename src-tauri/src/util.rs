//! Small process helpers shared across modules.

use std::process::Command;

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
