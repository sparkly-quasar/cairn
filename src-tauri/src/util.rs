//! Small process helpers shared across modules.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Emitter};

/// The user's home directory: `HOME` on Unix, `USERPROFILE` on Windows.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Build a `Command` with an enriched `PATH`. GUI apps launched from Finder or
/// the Dock inherit only launchd's bare `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`),
/// so tools installed by Homebrew (`/opt/homebrew/bin`) or `uv`/pipx
/// (`~/.local/bin`) — like `ollama`, `brew`, and `uv` — are invisible and every
/// spawn fails. Prepending the usual install dirs makes them resolvable whether
/// the app runs from a terminal (dev) or the Dock (bundled).
///
/// On Windows the equivalent problem is a PATH that was extended by an
/// installer (Ollama, uv) *after* Cairn started — the running process never
/// sees the update — so we prepend those tools' default install dirs too.
/// Console-window suppression: every helper we run (`nvidia-smi`, `taskkill`,
/// PowerShell, …) would otherwise flash a console window over the GUI, so all
/// commands are created with `CREATE_NO_WINDOW`.
pub fn command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.env("PATH", enriched_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// The inherited `PATH` with common tool install dirs prepended.
/// Non-existent dirs are harmless; existing entries are preserved after ours.
fn enriched_path() -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    #[cfg(not(target_os = "windows"))]
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
    #[cfg(target_os = "windows")]
    {
        // Per-user installer locations that an already-running Cairn won't have
        // in its inherited PATH yet.
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            dirs.push(local.join("Programs").join("Ollama"));
            dirs.push(local.join("Microsoft").join("WindowsApps")); // winget shims
        }
        dirs.push(PathBuf::from(r"C:\Program Files\Tailscale"));
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

/// Run a PowerShell one-liner (Windows helpers: hardware probes, port lookup).
/// Same contract as [`run`]: trimmed stdout on success, else `None`.
#[cfg(target_os = "windows")]
pub fn run_powershell(script: &str) -> Option<String> {
    run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )
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
