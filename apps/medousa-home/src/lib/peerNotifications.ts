import { ensureNotificationPermission, notifyPeerMessage } from "$lib/notifications";
import {
  listTrustedWorkshops,
  peerListMessages,
  type PeerMessage,
} from "$lib/utils/lanShareApi";
import { isTauri } from "$lib/window";

const seenMessageIds = new Set<string>();
let polling = false;
let seeded = false;

function deviceIdsMatch(left: string, right: string): boolean {
  if (!left || !right) return left === right;
  return (
    left === right ||
    left.startsWith(right.slice(0, 8)) ||
    right.startsWith(left.slice(0, 8))
  );
}

function isInboundUnread(message: PeerMessage): boolean {
  return message.direction !== "out" && !message.readAt;
}

function previewBody(body: string): string {
  const trimmed = body.trim();
  if (trimmed.length <= 120) return trimmed;
  return `${trimmed.slice(0, 117)}…`;
}

async function resolveWorkshopId(
  message: PeerMessage,
  trusted: Awaited<ReturnType<typeof listTrustedWorkshops>>,
): Promise<string | null> {
  if (message.workshopId?.trim()) {
    return message.workshopId.trim();
  }
  const fromId = message.fromDeviceId;
  const match = trusted.find(
    (peer) =>
      deviceIdsMatch(peer.workshopDeviceId, fromId) ||
      deviceIdsMatch(peer.workshopId, fromId),
  );
  return match?.workshopId ?? null;
}

export async function pollPeerMessageNotifications(): Promise<void> {
  if (!isTauri() || polling) return;
  polling = true;
  try {
    if (!(await ensureNotificationPermission())) return;
    const [messages, trusted] = await Promise.all([
      peerListMessages(false),
      listTrustedWorkshops(),
    ]);
    for (const message of messages) {
      if (!seeded) {
        seenMessageIds.add(message.id);
        continue;
      }
      if (!isInboundUnread(message)) continue;
      if (seenMessageIds.has(message.id)) continue;
      seenMessageIds.add(message.id);
      if (seenMessageIds.size > 256) {
        const oldest = seenMessageIds.values().next().value;
        if (oldest) seenMessageIds.delete(oldest);
      }
      const workshopId = await resolveWorkshopId(message, trusted);
      if (!workshopId) continue;
      await notifyPeerMessage({
        fromName: message.fromName,
        body: previewBody(message.body),
        workshopId,
        peerDeviceId: message.fromDeviceId,
        messageId: message.id,
      });
    }
    seeded = true;
  } catch {
    // Poll failures are non-fatal.
  } finally {
    polling = false;
  }
}

export function startPeerMessageNotificationPolling(
  intervalMs = 8000,
): () => void {
  if (!isTauri()) return () => {};
  void pollPeerMessageNotifications();
  const timer = setInterval(() => {
    void pollPeerMessageNotifications();
  }, intervalMs);
  return () => clearInterval(timer);
}
