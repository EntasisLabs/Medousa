let permissionReady: boolean | null = null;

async function notificationApi() {
  return import("@tauri-apps/plugin-notification");
}

async function ensurePermission(): Promise<boolean> {
  if (permissionReady !== null) return permissionReady;
  try {
    const { isPermissionGranted, requestPermission } = await notificationApi();
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

function notificationsEnabled(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem("medousa-home-notifications") !== "0";
}

export async function notifyCardDone(title: string, statusLabel: string) {
  try {
    if (!notificationsEnabled()) return;
    if (!(await ensurePermission())) return;
    const { sendNotification } = await notificationApi();
    sendNotification({
      title: "Medousa — work finished",
      body: `${title} · ${statusLabel}`,
    });
  } catch {
    // Vite-only dev or plugin unavailable — ignore.
  }
}
