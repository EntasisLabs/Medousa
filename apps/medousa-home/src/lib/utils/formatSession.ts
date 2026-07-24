import type { SessionSummary } from "$lib/types/session";

const UUID_LIKE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Human session label — preview line, not raw IDs. */
export function formatSessionLabel(session: SessionSummary): string {
  const named = session.display_name?.trim();
  if (named && !looksLikeId(named)) return named;

  const preview = session.preview.trim();
  if (preview) {
    const firstLine = preview.split("\n")[0].trim();
    if (firstLine && !looksLikeId(firstLine)) return truncate(firstLine, 48);
  }

  if (looksLikeId(session.session_id)) return "New conversation";
  return session.session_id;
}

type PresenceSlot = "past-midnight" | "morning" | "today" | "evening" | "tonight";

function presenceSlot(now = new Date()): PresenceSlot {
  const hour = now.getHours();
  if (hour < 5) return "past-midnight";
  if (hour < 12) return "morning";
  if (hour < 17) return "today";
  if (hour < 21) return "evening";
  return "tonight";
}

/** Soft room title for an empty Presence chat (time-of-day, not a UUID). */
export function presenceRoomTitle(now = new Date()): string {
  switch (presenceSlot(now)) {
    case "past-midnight":
      return "Past midnight";
    case "morning":
      return "Morning";
    case "today":
      return "Today";
    case "evening":
      return "Evening";
    case "tonight":
      return "Tonight";
  }
}

/**
 * Tab/header label for a chat session: Presence room title when the thread is
 * empty of chat/worker turns (asks live under Work and don't own the name).
 */
export function chatPresenceOrSessionLabel(
  session: SessionSummary,
  options?: {
    hasChatOrWorkerMessages?: boolean;
  },
): string {
  if (options?.hasChatOrWorkerMessages === false) return presenceRoomTitle();
  if (
    options?.hasChatOrWorkerMessages == null &&
    session.turns <= 0 &&
    !session.preview.trim() &&
    !session.display_name?.trim()
  ) {
    return presenceRoomTitle();
  }
  return formatSessionLabel(session);
}

/** The only empty-state ask — aligned with {@link presenceRoomTitle}. */
export function presenceSubline(now = new Date()): string {
  switch (presenceSlot(now)) {
    case "past-midnight":
      return "What are we doing past midnight?";
    case "morning":
      return "What are we doing this morning?";
    case "today":
      return "What are we doing today?";
    case "evening":
      return "What are we doing this evening?";
    case "tonight":
      return "What are we doing tonight?";
  }
}

/** Compact relative timestamp for session list rows. */
export function formatSessionWhen(iso?: string | null): string {
  if (!iso) return "";
  try {
    const date = new Date(iso);
    const diffMs = Date.now() - date.getTime();
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return "now";
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 48) return `${hours}h`;
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  } catch {
    return "";
  }
}

function looksLikeId(value: string): boolean {
  if (UUID_LIKE.test(value) || /^sess[_-]/i.test(value)) return true;
  // Daemon ids like `medousa-home-<uuid>` should never surface as titles.
  if (/^medousa-home-/i.test(value)) return true;
  if (/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)) {
    return true;
  }
  return false;
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1)}…`;
}
