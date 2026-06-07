import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let permissionReady: boolean | null = null;

async function ensurePermission(): Promise<boolean> {
  if (permissionReady !== null) return permissionReady;
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const result = await requestPermission();
      granted = result === "granted";
    }
    permissionReady = granted;
    return granted;
  } catch {
    permissionReady = false;
    return false;
  }
}

export async function notifyCardDone(title: string, statusLabel: string) {
  if (!(await ensurePermission())) return;
  sendNotification({
    title: "Medousa — work finished",
    body: `${title} · ${statusLabel}`,
  });
}
