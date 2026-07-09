// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
//! Engine management: the native Ollama runtime and the Open WebUI chat UI.
//! Open WebUI runs natively via `uv` (no Docker) — see `openwebui_native`.

pub mod ollama;
pub mod openwebui_native;
