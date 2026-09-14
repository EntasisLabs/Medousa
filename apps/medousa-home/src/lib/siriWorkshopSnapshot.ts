import { invoke } from "@tauri-apps/api/core";

export async function syncSiriWorkshopSnapshot(): Promise<void> {
  try {
    await invoke("siri_sync_workshop_snapshot");
  } catch (error) {
    console.warn("[siri] workshop snapshot sync failed:", error);
  }
}
