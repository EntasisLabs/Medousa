import { invoke } from "@tauri-apps/api/core";
import { isTauriIos } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";

const REMOTE_PUSH_KEY = "medousa-home-remote-push";
const TURN_UPDATES_KEY = "medousa-home-notify-turn-updates";
const NEEDS_INPUT_KEY = "medousa-home-notify-needs-input";
const PEER_MESSAGES_KEY = "medousa-home-notify-peer-messages";
const REMINDERS_KEY = "medousa-home-notify-reminders";

export interface NotificationPreferencePayload {
  turnUpdatesEnabled: boolean;
  needsInputEnabled: boolean;
  peerMessagesEnabled: boolean;
  remindersEnabled: boolean;
  remotePushEnabled: boolean;
}

export function remotePushEnabled(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(REMOTE_PUSH_KEY) === "1";
}

export function setRemotePushEnabled(
  enabled: boolean,
  preferences: NotificationPreferencePayload,
): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(REMOTE_PUSH_KEY, enabled ? "1" : "0");
  if (!enabled) {
    void invoke("push_clear_apns_token").catch(() => {});
    void sendPairingHeartbeat({
      ...preferences,
      remotePushEnabled: false,
      apnsDeviceToken: "",
      pushPlatform: "ios",
    }).catch(() => {});
  } else {
    void sendPairingHeartbeat(preferences).catch(() => {});
    void registerRemotePush();
  }
}

export function syncNotificationPreferences(preferences: NotificationPreferencePayload): void {
  void sendPairingHeartbeat(preferences).catch(() => {});
}

function readEnabled(key: string, fallback: boolean): boolean {
  if (typeof localStorage === "undefined") return fallback;
  const stored = localStorage.getItem(key);
  return stored === "1" ? true : stored === "0" ? false : fallback;
}

function currentPreferences(): NotificationPreferencePayload {
  return {
    turnUpdatesEnabled: readEnabled(TURN_UPDATES_KEY, false),
    needsInputEnabled: readEnabled(NEEDS_INPUT_KEY, true),
    peerMessagesEnabled: readEnabled(PEER_MESSAGES_KEY, true),
    remindersEnabled: readEnabled(REMINDERS_KEY, true),
    remotePushEnabled: remotePushEnabled(),
  };
}

async function storeApnsToken(token: string): Promise<void> {
  const trimmed = token.trim();
  if (!trimmed) return;
  await invoke("push_register_apns_token", { token: trimmed });
  await sendPairingHeartbeat({
    ...currentPreferences(),
    apnsDeviceToken: trimmed,
    pushPlatform: "ios",
  });
}

export async function registerRemotePush(): Promise<boolean> {
  if (!isTauriIos() || !remotePushEnabled()) return false;

  try {
    const { requestPermission, getToken } = await import("tauri-plugin-mobile-push-api");
    const { granted } = await requestPermission();
    if (!granted) return false;

    const token = (await getToken()).trim();
    if (!token) return false;

    await storeApnsToken(token);

    try {
      const { onTokenRefresh } = await import("tauri-plugin-mobile-push-api");
      await onTokenRefresh(({ token: refreshed }) => {
        void storeApnsToken(refreshed);
      });
    } catch {
      // Token refresh listener is best-effort on iOS.
    }

    return true;
  } catch (err) {
    console.warn("[mobile-push] get_token failed:", err);
    return false;
  }
}
