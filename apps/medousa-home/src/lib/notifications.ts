import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import type { OpenWorkHandler } from "$lib/mobileNative";
import { isTauriMobilePlatform } from "$lib/platform";
import type { HomeNotificationIntent } from "$lib/types/workspace";
import { shouldPresentHomeNotification } from "$lib/homeNotificationPolicy";

let permissionReady: boolean | null = null;

/** macOS notification APIs are not safe under concurrent tokio worker calls — serialize. */
const NOTIFICATION_MIN_GAP_MS = 300;
let notificationChain: Promise<void> = Promise.resolve();
let lastNotificationSentAt = 0;

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

const NOTIFICATION_LEDGER_KEY = "medousa-home-notification-ledger-v1";
const NOTIFICATION_LEDGER_LIMIT = 512;

function categoryEnabled(intent: HomeNotificationIntent): boolean {
  return shouldPresentHomeNotification(intent.kind, {
    turnUpdates:
      typeof localStorage !== "undefined" &&
      localStorage.getItem("medousa-home-notify-turn-updates") === "1",
    needsInput:
      typeof localStorage === "undefined" ||
      localStorage.getItem("medousa-home-notify-needs-input") !== "0",
  });
}

function loadNotificationLedger(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const parsed = JSON.parse(localStorage.getItem(NOTIFICATION_LEDGER_KEY) ?? "[]");
    return Array.isArray(parsed)
      ? parsed.filter((value): value is string => typeof value === "string")
      : [];
  } catch {
    return [];
  }
}

function rememberNotificationId(notificationId: string): boolean {
  if (typeof localStorage === "undefined") return true;
  const ledger = loadNotificationLedger();
  if (ledger.includes(notificationId)) return false;
  ledger.push(notificationId);
  localStorage.setItem(
    NOTIFICATION_LEDGER_KEY,
    JSON.stringify(ledger.slice(-NOTIFICATION_LEDGER_LIMIT)),
  );
  return true;
}

/** Present a daemon-authored notification intent without reinterpreting cards or turns. */
export async function presentHomeNotification(intent: HomeNotificationIntent): Promise<void> {
  if (!intent.notification_id.trim() || !categoryEnabled(intent)) return;
  // On iOS, Remote push is the delivery route when enabled. Avoid presenting
  // the same daemon intent again from the live workspace stream.
  if (
    isTauriMobilePlatform() &&
    typeof localStorage !== "undefined" &&
    localStorage.getItem("medousa-home-remote-push") === "1"
  ) return;
  if (!(await ensureNotificationPermission())) return;
  if (!rememberNotificationId(intent.notification_id)) return;

  enqueueNotification(async () => {
    const { sendNotification } = await notificationApi();
    const cardId = intent.card_id?.trim() || intent.subject_id.trim();
    sendNotification({
      id: notificationId(intent.notification_id),
      title: intent.title,
      body: intent.body,
      actionTypeId: "medousa-work",
      extra: { cardId, kind: "work" } satisfies WorkNotificationExtra,
    });
  });
}

type WorkNotificationExtra = {
  cardId: string;
  kind: "work";
};

type PeerNotificationExtra = {
  kind: "peer";
  workshopId: string;
  peerDeviceId?: string;
  messageId?: string;
};

const peerNotified = new Set<string>();

function notificationId(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) || 1;
}

function rememberOnce(set: Set<string>, key: string, limit = 256): boolean {
  if (set.has(key)) return false;
  set.add(key);
  if (set.size > limit) {
    const oldest = set.values().next().value;
    if (oldest) set.delete(oldest);
  }
  return true;
}

function rememberPeerNotification(seed: string): boolean {
  return rememberOnce(peerNotified, seed, 256);
}

async function sendPeerNotification(
  seed: string,
  title: string,
  body: string,
  extra: PeerNotificationExtra,
) {
  if (
    typeof localStorage !== "undefined" &&
    localStorage.getItem("medousa-home-notify-peer-messages") === "0"
  ) return;
  if (!rememberPeerNotification(seed)) return;
  if (!(await ensureNotificationPermission())) return;

  enqueueNotification(async () => {
    const { sendNotification } = await notificationApi();
    sendNotification({
      id: notificationId(seed),
      title,
      body,
      actionTypeId: "medousa-peer",
      extra,
    });
  });
}

export async function notifyPeerMessage(input: {
  fromName: string;
  body: string;
  workshopId: string;
  peerDeviceId?: string;
  messageId?: string;
}) {
  try {
    const preview = input.body.trim() || "New message";
    await sendPeerNotification(
      `peer-${input.messageId ?? `${input.workshopId}-${Date.now()}`}`,
      `Medousa — ${input.fromName}`,
      preview,
      {
        kind: "peer",
        workshopId: input.workshopId,
        peerDeviceId: input.peerDeviceId,
        messageId: input.messageId,
      },
    );
  } catch {
    // Vite-only dev or plugin unavailable — ignore.
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function enqueueNotification(task: () => Promise<void>): void {
  notificationChain = notificationChain
    .then(async () => {
      const elapsed = Date.now() - lastNotificationSentAt;
      const wait = NOTIFICATION_MIN_GAP_MS - elapsed;
      if (wait > 0) {
        await sleep(wait);
      }
      await task();
      lastNotificationSentAt = Date.now();
    })
    .catch(() => {
      // Keep the queue alive after a failed notification.
    });
}

/** Workspace card id for a turn budget request (notification tap + work board). */
export function budgetWorkCardId(requestId: string): string {
  return requestId.trim();
}

export function budgetRequestIdFromStreamEvent(
  event: InteractiveTurnStreamEvent | TurnStreamEnvelopeV3,
): string | null {
  if ("event" in event) {
    return event.event.type === "budget_approval_required"
      ? budgetWorkCardId(event.event.request_id)
      : null;
  }
  const explicit = event.budget_request_id?.trim();
  if (explicit) return budgetWorkCardId(explicit);
  const match = event.message.match(/\(request ([^)]+)\)/);
  const parsed = match?.[1]?.trim();
  return parsed ? budgetWorkCardId(parsed) : null;
}

function cardIdFromNotification(extra: unknown): string | null {
  if (!extra || typeof extra !== "object") return null;
  const record = extra as Record<string, unknown>;
  if (record.kind !== "work") return null;
  const cardId = record.cardId;
  return typeof cardId === "string" && cardId.trim() ? cardId.trim() : null;
}

function peerTargetFromNotification(extra: unknown): PeerNotificationExtra | null {
  if (!extra || typeof extra !== "object") return null;
  const record = extra as Record<string, unknown>;
  if (record.kind !== "peer") return null;
  const workshopId = record.workshopId;
  if (typeof workshopId !== "string" || !workshopId.trim()) return null;
  return {
    kind: "peer",
    workshopId: workshopId.trim(),
    peerDeviceId:
      typeof record.peerDeviceId === "string" ? record.peerDeviceId.trim() : undefined,
    messageId: typeof record.messageId === "string" ? record.messageId.trim() : undefined,
  };
}

function calendarUidFromNotification(extra: unknown): string | null {
  if (!extra || typeof extra !== "object") return null;
  const record = extra as Record<string, unknown>;
  if (record.kind !== "calendar") return null;
  const uid = record.uid;
  return typeof uid === "string" && uid.trim() ? uid.trim() : null;
}

export type OpenPeerHandler = (input: {
  workshopId: string;
  peerDeviceId?: string;
  messageId?: string;
}) => void | Promise<void>;

export type OpenCalendarHandler = (uid: string) => void | Promise<void>;

/** Wire notification taps to work-card, peer-thread, and calendar navigation. */
export async function initNotificationRouting(
  onOpenWork: OpenWorkHandler,
  onOpenPeer?: OpenPeerHandler,
  onOpenCalendar?: OpenCalendarHandler,
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
      {
        id: "medousa-peer",
        actions: [
          {
            id: "open",
            title: "Open",
            foreground: true,
          },
        ],
      },
      {
        id: "medousa-calendar",
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
      if (cardId) {
        void onOpenWork(cardId);
        return;
      }
      const peer = peerTargetFromNotification(notification.extra);
      if (peer && onOpenPeer) {
        void onOpenPeer(peer);
        return;
      }
      const calendarUid = calendarUidFromNotification(notification.extra);
      if (calendarUid && onOpenCalendar) {
        void onOpenCalendar(calendarUid);
      }
    });
    return () => {
      void listener.unregister();
    };
  } catch {
    return null;
  }
}
