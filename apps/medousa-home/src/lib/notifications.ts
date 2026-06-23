import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { OpenWorkHandler } from "$lib/mobileNative";
import { isTauriMobilePlatform } from "$lib/platform";

let permissionReady: boolean | null = null;
const budgetNotified = new Set<string>();
const workNotified = new Set<string>();

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

function rememberOnce(set: Set<string>, key: string, limit = 256): boolean {
  if (set.has(key)) return false;
  set.add(key);
  if (set.size > limit) {
    const oldest = set.values().next().value;
    if (oldest) set.delete(oldest);
  }
  return true;
}

function rememberBudgetNotification(requestId: string): boolean {
  return rememberOnce(budgetNotified, requestId, 128);
}

function rememberWorkNotification(seed: string): boolean {
  return rememberOnce(workNotified, seed, 256);
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

async function sendWorkNotification(
  seed: string,
  title: string,
  body: string,
  cardId: string,
) {
  if (!notificationsEnabled()) return;
  if (!rememberWorkNotification(seed)) return;
  if (!(await ensureNotificationPermission())) return;

  enqueueNotification(async () => {
    const { sendNotification } = await notificationApi();
    sendNotification({
      id: notificationId(seed),
      title,
      body,
      actionTypeId: "medousa-work",
      extra: { cardId, kind: "work" } satisfies WorkNotificationExtra,
    });
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

/** Workspace card id for a turn budget request (notification tap + work board). */
export function budgetWorkCardId(requestId: string): string {
  return requestId.trim();
}

/** Local push on iOS/Android when a turn pauses for tool-round budget approval. */
export async function notifyBudgetApprovalRequired(
  title: string,
  requestId: string,
  detail?: string,
) {
  if (!isTauriMobilePlatform()) return;
  const trimmedId = budgetWorkCardId(requestId);
  if (!trimmedId || !rememberBudgetNotification(trimmedId)) return;

  const summary = detail?.trim() || title.trim() || "Turn needs more tool rounds";
  const body =
    summary.length > 160 ? `${summary.slice(0, 157)}…` : summary;

  try {
    await sendWorkNotification(
      `budget-${trimmedId}`,
      "Medousa — approve more rounds?",
      `${body} · tap to review`,
      trimmedId,
    );
  } catch {
    // Vite-only dev or plugin unavailable — ignore.
  }
}

export function budgetRequestIdFromStreamEvent(
  event: InteractiveTurnStreamEvent,
): string | null {
  const explicit = event.budget_request_id?.trim();
  if (explicit) return budgetWorkCardId(explicit);
  const match = event.message.match(/\(request ([^)]+)\)/);
  const parsed = match?.[1]?.trim();
  return parsed ? budgetWorkCardId(parsed) : null;
}

const turnTerminalNotified = new Set<string>();

export async function notifyTurnTicketTerminal(
  event: InteractiveTurnStreamEvent,
  workspaceCardId?: string | null,
) {
  if (!isTauriMobilePlatform() || !event.terminal) return;
  const turnId = event.turn_id.trim();
  if (!turnId || turnTerminalNotified.has(turnId)) return;
  turnTerminalNotified.add(turnId);
  if (turnTerminalNotified.size > 128) {
    const oldest = turnTerminalNotified.values().next().value;
    if (oldest) turnTerminalNotified.delete(oldest);
  }

  const cardId = workspaceCardId?.trim() || turnId;
  const preview =
    event.operator_message?.trim() ||
    event.final_text?.trim().split("\n")[0]?.trim() ||
    event.message?.trim() ||
    "Turn finished";

  if (event.event_type === "error") {
    try {
      await sendWorkNotification(
        `turn-error-${turnId}`,
        "Medousa — turn failed",
        preview.length > 120 ? `${preview.slice(0, 117)}…` : preview,
        cardId,
      );
    } catch {
      // ignore
    }
    return;
  }

  try {
    await sendWorkNotification(
      `turn-done-${turnId}`,
      "Medousa — turn ready",
      preview.length > 120 ? `${preview.slice(0, 117)}…` : preview,
      cardId,
    );
  } catch {
    // ignore
  }
}

export async function notifyWorkerHandoff(
  event: InteractiveTurnStreamEvent,
  workspaceCardId?: string | null,
) {
  if (!isTauriMobilePlatform() || event.event_type !== "worker_ack") return;
  const cardId = workspaceCardId?.trim() || event.turn_id.trim();
  if (!cardId) return;
  try {
    await sendWorkNotification(
      `worker-${event.turn_id}`,
      "Medousa — worker started",
      event.message?.trim() || "Background worker is on it",
      cardId,
    );
  } catch {
    // ignore
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
