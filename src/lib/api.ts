// SPDX-License-Identifier: Apache-2.0
// Typed wrappers around the Tauri command surface (see src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SystemProfile {
  os: string;
  arch: string;
  ram_gb: number;
  gpu_vendor: "apple" | "nvidia" | "amd" | "none";
  gpu_name: string | null;
  vram_gb: number;
  cpu_cores: number;
  free_disk_gb: number;
  docker_present: boolean;
  ollama_present: boolean;
  gpu_experimental: boolean;
}

export interface Recommendation {
  model_id: string;
  display_name: string;
  ollama_tag: string;
  disk_gb: number;
  min_ram_gb: number;
  rating: "green" | "yellow" | "red";
  rating_label: string;
  reason: string;
}

export interface Ack {
  headline: string;
  points: string[];
}

export interface RatedModel {
  id: string;
  display_name: string;
  ollama_tag: string;
  params: string;
  disk_gb: number;
  min_ram_gb: number;
  capabilities: string[];
  use_cases: string[];
  blurb: string;
  license: string;
  library_url: string;
  requires_ack: Ack | null;
  rating: "green" | "yellow" | "red";
  rating_label: string;
  reason: string;
}

export interface Bundle {
  id: string;
  icon: string;
  title: string;
  blurb: string;
}

export type BindTier = "private" | "lan" | "tailscale";

export interface TailscaleInfo {
  installed: boolean;
  running: boolean;
  ipv4: string | null;
  dns_name: string | null;
}

export interface ServerStatus {
  tier: BindTier;
  running: boolean;
  port: number | null;
  private_url: string | null;
  lan_ip: string | null;
  lan_url: string | null;
  tailscale: TailscaleInfo;
  tailscale_url: string | null;
}

export const detectSystem = () => invoke<SystemProfile>("detect_system");
export const getRecommendation = () => invoke<Recommendation>("get_recommendation");
export const getCatalog = () => invoke<RatedModel[]>("get_catalog");
export const getBundles = () => invoke<Bundle[]>("get_bundles");
export const isModelPresent = (tag: string) => invoke<boolean>("is_model_present", { tag });
export const dockerRunning = () => invoke<boolean>("docker_running");
export const installOllama = () => invoke<void>("install_ollama");
export const pullModel = (tag: string) => invoke<void>("pull_model", { tag });
export const ensureOpenWebui = () => invoke<string>("ensure_openwebui");
export const serverStatus = () => invoke<ServerStatus>("server_status");
export const setServerTier = (tier: BindTier) => invoke<ServerStatus>("set_server_tier", { tier });
export const qrSvg = (text: string) => invoke<string>("qr_svg", { text });
export const openChat = (url: string) => invoke<void>("open_chat", { url });

/** Subscribe to a streamed backend progress event. */
export const onProgress = (
  event: "ollama-install-progress" | "pull-progress",
  cb: (line: string) => void,
): Promise<UnlistenFn> => listen<string>(event, (e) => cb(e.payload));
