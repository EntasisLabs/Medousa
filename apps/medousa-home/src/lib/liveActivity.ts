import { invoke } from "@tauri-apps/api/core";
import type { DaemonHealth } from "$lib/daemon";
import type { WorkCard } from "$lib/types/workspace";
import { isTauriIos } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";
import {
  buildPulsePresentation,
  motionColumnCounts,
  type PulseMood,
} from "$lib/utils/mobilePulse";

const LIVE_ACTIVITY_KEY = "medousa-home-live-activity";

export interface LiveActivityPayload {
  mood: PulseMood;
  workshopName: string;
  eyebrow: string;
  headline: string;
  subline?: string;
  motionSummary?: string;
  blockedCount: number;
  primaryCardId?: string;
}

export interface LiveActivityStatus {
  available: boolean;
  active: boolean;
  error?: string;
  pushToken?: string;
  diagnostics?: {
    bridgeLinked: boolean;
    activitiesEnabled: boolean;
    widgetExtensionInstalled: boolean;
    supportsLiveActivities: boolean;
    error?: string;
  };
}

let lastPayloadKey = "";
let lastRegisteredPushToken = "";
let syncInFlight = false;
let pushTokenPollTimer: ReturnType<typeof setTimeout> | null = null;

export function liveActivityEnabled(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(LIVE_ACTIVITY_KEY) === "1";
}

export function setLiveActivityEnabled(enabled: boolean): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(LIVE_ACTIVITY_KEY, enabled ? "1" : "0");
}

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

export function buildLiveActivityPayload(input: {
  health: DaemonHealth | null;
  cards: WorkCard[];
  blocked: number;
  inMotion: number;
  primaryCard: WorkCard | null;
  workshopName: string;
  journalDailyPath?: string | null;
  journalDailyTitle?: string | null;
}): LiveActivityPayload {
  const healthOk = input.health === null ? null : input.health.ok;
  const pulse = buildPulsePresentation({
    healthOk,
    blocked: input.blocked,
    inMotion: input.inMotion,
    primaryCard: input.primaryCard,
    motionCounts: motionColumnCounts(input.cards),
    journalDailyPath: input.journalDailyPath,
    journalDailyTitle: input.journalDailyTitle,
  });

  return {
    mood: pulse.mood,
    workshopName: input.workshopName,
    eyebrow: pulse.eyebrow,
    headline: pulse.headline,
    subline: pulse.subline,
    motionSummary: pulse.motionSummary,
    blockedCount: input.blocked,
    primaryCardId:
      pulse.action.kind === "card" ? pulse.action.cardId : input.primaryCard?.id,
  };
}

async function readPushToken(): Promise<string | null> {
  try {
    const token = await invoke<string | null>("live_activity_push_token");
    const trimmed = token?.trim();
    return trimmed ? trimmed : null;
  } catch {
    return null;
  }
}

async function registerPushTokenIfNeeded(token: string | null): Promise<void> {
  if (!token || token === lastRegisteredPushToken) return;
  lastRegisteredPushToken = token;
  try {
    await sendPairingHeartbeat({ liveActivityPushToken: token });
  } catch (err) {
    console.warn("[live-activity] heartbeat push token:", err);
  }
}

function clearPushTokenPoll(): void {
  if (pushTokenPollTimer !== null) {
    clearTimeout(pushTokenPollTimer);
    pushTokenPollTimer = null;
  }
}

function schedulePushTokenPoll(attempt = 0): void {
  clearPushTokenPoll();
  if (!liveActivityEnabled() || attempt > 8) return;

  pushTokenPollTimer = setTimeout(() => {
    void (async () => {
      const token = await readPushToken();
      if (token) {
        await registerPushTokenIfNeeded(token);
        return;
      }
      schedulePushTokenPoll(attempt + 1);
    })();
  }, attempt === 0 ? 400 : 800);
}

async function afterLiveActivitySync(status: LiveActivityStatus | null): Promise<void> {
  if (!status?.active) {
    clearPushTokenPoll();
    if (lastRegisteredPushToken) {
      lastRegisteredPushToken = "";
      void sendPairingHeartbeat({ liveActivityPushToken: "" }).catch(() => {});
    }
    return;
  }

  const token = status.pushToken?.trim() || (await readPushToken());
  if (token) {
    await registerPushTokenIfNeeded(token);
    return;
  }
  schedulePushTokenPoll();
}

export async function syncLiveActivity(
  payload: LiveActivityPayload,
): Promise<LiveActivityStatus | null> {
  if (!isTauriIos() || !liveActivityEnabled()) return null;

  const key = payloadKey(payload);
  if (key === lastPayloadKey || syncInFlight) return null;
  lastPayloadKey = key;
  syncInFlight = true;

  try {
    const status = await invoke<LiveActivityStatus>("live_activity_sync", { payload });
    if (status.error) {
      console.warn("[live-activity] sync:", status.error);
    }
    await afterLiveActivitySync(status);
    return status;
  } catch (err) {
    console.warn("[live-activity] invoke failed:", err);
    return null;
  } finally {
    syncInFlight = false;
  }
}

export async function queryLiveActivityAvailability(): Promise<LiveActivityStatus | null> {
  if (!isTauriIos()) return null;
  try {
    return await invoke<LiveActivityStatus>("live_activity_is_available");
  } catch {
    return null;
  }
}

/** Reset dedupe so the next sync always runs (e.g. after toggling the setting). */
export function resetLiveActivitySync(): void {
  lastPayloadKey = "";
  lastRegisteredPushToken = "";
  clearPushTokenPoll();
}
