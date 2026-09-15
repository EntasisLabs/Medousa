import { invoke } from "@tauri-apps/api/core";
import { chat } from "$lib/stores/chat.svelte";
import { prepareInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { syncSiriPreferences } from "$lib/config/siriPreferences";

export async function syncSiriWorkshopSnapshot(): Promise<void> {
  try {
    await invoke("siri_sync_workshop_snapshot");
  } catch (error) {
    console.warn("[siri] workshop snapshot sync failed:", error);
  }
  await syncSiriPreferences();
  await syncSiriExecutionContext();
}

export async function syncSiriExecutionContext(): Promise<void> {
  if (!chat.sessionId.trim() || !chat.workshopScopeId) return;
  try {
    const options = await prepareInteractiveTurnOptions(chat);
    await invoke("siri_sync_execution_context", {
      context: {
        sessionId: chat.sessionId,
        provider: options.provider ?? "",
        model: options.model ?? "",
        responseDepthMode: options.responseDepthMode ?? "standard",
        reasoningEffort: options.reasoningEffort ?? "default",
        identityUserId: options.identityUserId ?? null,
      },
    });
  } catch {
    // Best-effort snapshot. Siri reports an unavailable workshop if this is stale.
  }
}
