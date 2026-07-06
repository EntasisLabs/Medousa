import {
  listTrustedWorkshops,
  peerListMessages,
  type PeerMessage,
  type TrustedWorkshopSummary,
} from "$lib/utils/lanShareApi";
import { isTauri } from "$lib/window";

const RECENT_MS = 7 * 24 * 60 * 60 * 1000;
const STRIP_LIMIT = 3;
const PREVIEW_MAX = 72;

export interface PeerThreadPreview {
  workshopId: string;
  label: string;
  workshopDeviceId: string;
  unreadCount: number;
  lastMessage: {
    body: string;
    sentAt: string;
    direction: "in" | "out" | string;
    fromName: string;
  } | null;
}

export interface PeerHomePreview {
  unreadTotal: number;
  peerCount: number;
  /** Top threads for the home strip (unread or recent). */
  stripThreads: PeerThreadPreview[];
  /** Most recent thread across all peers (for shortcut card hint). */
  latestThread: PeerThreadPreview | null;
}

const EMPTY_PREVIEW: PeerHomePreview = {
  unreadTotal: 0,
  peerCount: 0,
  stripThreads: [],
  latestThread: null,
};

function deviceIdsMatch(left: string, right: string): boolean {
  if (!left || !right) return left === right;
  return (
    left === right ||
    left.startsWith(right.slice(0, 8)) ||
    right.startsWith(left.slice(0, 8))
  );
}

function matchesPeer(message: PeerMessage, deviceId: string): boolean {
  if (deviceIdsMatch(message.fromDeviceId, deviceId)) return true;
  if (message.toDeviceId && deviceIdsMatch(message.toDeviceId, deviceId)) return true;
  return false;
}

function isOutbound(message: PeerMessage): boolean {
  return message.direction === "out";
}

function previewBody(body: string): string {
  const trimmed = body.trim();
  if (trimmed.length <= PREVIEW_MAX) return trimmed;
  return `${trimmed.slice(0, PREVIEW_MAX - 1)}…`;
}

function sentAtMs(sentAt: string): number {
  const ms = Date.parse(sentAt);
  return Number.isFinite(ms) ? ms : 0;
}

function buildThreadPreview(
  peer: TrustedWorkshopSummary,
  messages: PeerMessage[],
): PeerThreadPreview {
  const threadMessages = messages.filter((message) =>
    matchesPeer(message, peer.workshopDeviceId),
  );
  let unreadCount = 0;
  let latest: PeerMessage | null = null;
  let latestMs = 0;

  for (const message of threadMessages) {
    if (!isOutbound(message) && !message.readAt) unreadCount += 1;
    const ms = sentAtMs(message.sentAt);
    if (!latest || ms >= latestMs) {
      latest = message;
      latestMs = ms;
    }
  }

  return {
    workshopId: peer.workshopId,
    label: peer.label,
    workshopDeviceId: peer.workshopDeviceId,
    unreadCount,
    lastMessage: latest
      ? {
          body: latest.body,
          sentAt: latest.sentAt,
          direction: latest.direction ?? "in",
          fromName: latest.fromName,
        }
      : null,
  };
}

function isRecentThread(thread: PeerThreadPreview, nowMs: number): boolean {
  if (thread.unreadCount > 0) return true;
  if (!thread.lastMessage) return false;
  return nowMs - sentAtMs(thread.lastMessage.sentAt) <= RECENT_MS;
}

export function formatPeerRelativeTime(sentAt: string): string {
  try {
    const date = new Date(sentAt);
    const diffMs = Date.now() - date.getTime();
    if (diffMs < 60_000) return "Now";
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 48) return `${hours}h`;
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  } catch {
    return "";
  }
}

export function peerThreadPreviewLine(thread: PeerThreadPreview): string {
  if (!thread.lastMessage) return "No messages yet";
  const body = previewBody(thread.lastMessage.body);
  if (thread.lastMessage.direction === "out") return `You: ${body}`;
  return body;
}

export function peerHomeCardHint(preview: PeerHomePreview): string {
  if (preview.unreadTotal > 0) {
    return preview.unreadTotal === 1 ? "1 unread" : `${preview.unreadTotal} unread`;
  }
  if (preview.latestThread?.lastMessage) {
    return peerThreadPreviewLine(preview.latestThread);
  }
  if (preview.peerCount > 0) {
    return preview.peerCount === 1 ? "1 connection" : `${preview.peerCount} connections`;
  }
  return "Connect on LAN";
}

export async function fetchPeerHomePreview(): Promise<PeerHomePreview> {
  if (!isTauri()) return EMPTY_PREVIEW;
  try {
    const [messages, trusted] = await Promise.all([
      peerListMessages(false),
      listTrustedWorkshops(),
    ]);
    const threads = trusted.map((peer) => buildThreadPreview(peer, messages));
    const nowMs = Date.now();
    const unreadTotal = threads.reduce((sum, thread) => sum + thread.unreadCount, 0);
    const sorted = [...threads].sort((left, right) => {
      const leftMs = left.lastMessage ? sentAtMs(left.lastMessage.sentAt) : 0;
      const rightMs = right.lastMessage ? sentAtMs(right.lastMessage.sentAt) : 0;
      return rightMs - leftMs;
    });
    const stripThreads = sorted
      .filter((thread) => isRecentThread(thread, nowMs))
      .slice(0, STRIP_LIMIT);
    const latestThread = sorted.find((thread) => thread.lastMessage) ?? null;

    return {
      unreadTotal,
      peerCount: trusted.length,
      stripThreads,
      latestThread,
    };
  } catch {
    return EMPTY_PREVIEW;
  }
}
