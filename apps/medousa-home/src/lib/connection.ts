import { invoke } from "@tauri-apps/api/core";
import type { DaemonStartResult } from "$lib/utils/providersApi";
import { isTauri } from "$lib/window";

export interface ConnectionPrefsSummary {
  publicBind: boolean;
  autostartEnabled: boolean;
  autostartSupported: boolean;
}

export async function loadConnectionPrefs(): Promise<ConnectionPrefsSummary> {
  if (!isTauri()) {
    return {
      publicBind: false,
      autostartEnabled: false,
      autostartSupported: false,
    };
  }
  return invoke<ConnectionPrefsSummary>("connection_load_prefs");
}

export async function setPublicBind(enabled: boolean): Promise<DaemonStartResult> {
  if (!isTauri()) {
    throw new Error("Connection settings require the Medousa desktop app");
  }
  return invoke<DaemonStartResult>("connection_set_public_bind", { request: { enabled } });
}

export async function setAutostart(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  await invoke("connection_set_autostart", { request: { enabled } });
}
