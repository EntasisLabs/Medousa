import { invoke } from "@tauri-apps/api/core";
import { isTauriIos } from "$lib/platform";
import { sendPairingHeartbeat } from "$lib/utils/pairingClient";

const REMOTE_PUSH_KEY = "medousa-home-remote-push";

export function remotePushEnabled(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem(REMOTE_PUSH_KEY) !== "0";
}

export function setRemotePushEnabled(enabled: boolean): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(REMOTE_PUSH_KEY, enabled ? "1" : "0");
  if (!enabled) {
    void invoke("push_clear_apns_token").catch(() => {});
  } else {
    void registerRemotePush();
  }
}

async function storeApnsToken(token: string): Promise<void> {
  const trimmed = token.trim();
  if (!trimmed) return;
  await invoke("push_register_apns_token", { token: trimmed });
  await sendPairingHeartbeat({ apnsDeviceToken: trimmed, pushPlatform: "ios" });
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
  } catch {
    return false;
  }
}
