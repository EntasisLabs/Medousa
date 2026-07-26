import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface AppUpdateStatus {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  downloadUrl: string | null;
  releaseBaseUrl: string | null;
  channel: string;
  error: string | null;
}

export async function fetchAppUpdateStatus(): Promise<AppUpdateStatus | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<AppUpdateStatus>("app_update_status");
  } catch {
    return null;
  }
}

export async function openAppUpdateDownload(): Promise<void> {
  await invoke("app_update_open_download");
}
