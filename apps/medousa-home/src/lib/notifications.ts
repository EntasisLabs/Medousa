import type { OpenWorkHandler } from "$lib/mobileNative";

let permissionReady: boolean | null = null;

async function notificationApi() {
  return import("@tauri-apps/plugin-notification");
}

export async function ensureNotificationPermission(): Promise<boolean> {
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

type WorkNotificationExtra = {
  cardId: string;
  kind: "work";
};

function notificationId(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) || 1;
}

async function sendWorkNotification(
  seed: string,
  title: string,
  body: string,
  cardId: string,
) {
  if (!notificationsEnabled()) return;
  if (!(await ensureNotificationPermission())) return;
  const { sendNotification } = await notificationApi();
  sendNotification({
    id: notificationId(seed),
    title,
    body,
    actionTypeId: "medousa-work",
    extra: { cardId, kind: "work" } satisfies WorkNotificationExtra,
  });
}

export async function notifyCardDone(
  title: string,
  statusLabel: string,
  cardId: string,
) {
  try {
    await sendWorkNotification(
      `work-done-${cardId}`,
      "Medousa — work finished",
      `${title} · ${statusLabel}`,
      cardId,
    );
  } catch {
    // Vite-only dev or plugin unavailable — ignore.
  }
}

export async function notifyAskComplete(title: string, cardId: string) {
  try {
    await sendWorkNotification(
      `work-ask-${cardId}`,
      "Medousa — ask ready",
      `${title} · tap to read the result`,
      cardId,
    );
  } catch {
    // Vite-only dev or plugin unavailable — ignore.
  }
}

function cardIdFromNotification(extra: unknown): string | null {
  if (!extra || typeof extra !== "object") return null;
  const record = extra as Record<string, unknown>;
  if (record.kind !== "work") return null;
  const cardId = record.cardId;
  return typeof cardId === "string" && cardId.trim() ? cardId.trim() : null;
}

/** Wire notification taps to work-card navigation (Tauri mobile + desktop). */
export async function initNotificationRouting(
  onOpenWork: OpenWorkHandler,
): Promise<(() => void) | null> {
  try {
    const { registerActionTypes, onAction } = await notificationApi();
    await registerActionTypes([
      {
        id: "medousa-work",
        actions: [
          {
            id: "open",
            title: "Open",
            foreground: true,
          },
        ],
      },
    ]);

    const listener = await onAction((notification) => {
      const cardId = cardIdFromNotification(notification.extra);
      if (cardId) void onOpenWork(cardId);
    });
    return () => {
      void listener.unregister();
    };
  } catch {
    return null;
  }
}
