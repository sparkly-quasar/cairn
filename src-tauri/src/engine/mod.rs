// SPDX-License-Identifier: Apache-2.0
//! Engine management: the native Ollama runtime and the Open WebUI chat UI.

pub mod ollama;
pub mod openwebui;

/// Whether the Docker CLI is installed (independent of whether the daemon runs).
pub fn docker_present() -> bool {
    crate::util::run("docker", &["--version"]).is_some()
}

/// Whether the Docker daemon is reachable (required to launch Open WebUI).
pub fn docker_running() -> bool {
    crate::util::run("docker", &["info", "--format", "{{.ServerVersion}}"]).is_some()
}
