import { invoke } from "@tauri-apps/api/core";
import type { LiveActivityPayload } from "$lib/liveActivity";
import { isTauriIos } from "$lib/platform";

export interface HomeWidgetSyncResult {
  ok: boolean;
  error?: string;
}

let lastPayloadKey = "";
let syncInFlight = false;

function payloadKey(payload: LiveActivityPayload): string {
  return [
    payload.mood,
    payload.workshopName,
    payload.eyebrow,
    payload.headline,
    payload.subline ?? "",
    payload.motionSummary ?? "",
    payload.blockedCount,
    payload.primaryCardId ?? "",
  ].join("|");
}

export function bumpHomeWidgetSync(): void {
  lastPayloadKey = "";
}

export async function syncHomeWidget(
  payload: LiveActivityPayload,
  options?: { force?: boolean },
): Promise<HomeWidgetSyncResult | null> {
  if (!isTauriIos()) return null;

  const key = payloadKey(payload);
  if (!options?.force && (key === lastPayloadKey || syncInFlight)) return null;
  lastPayloadKey = key;
  syncInFlight = true;

  try {
    const result = await invoke<HomeWidgetSyncResult>("home_widget_sync", { payload });
    if (result.error) {
      console.warn("[home-widget] sync:", result.error);
    }
    return result;
  } catch (err) {
    console.warn("[home-widget] invoke failed:", err);
    return null;
  } finally {
    syncInFlight = false;
  }
}
