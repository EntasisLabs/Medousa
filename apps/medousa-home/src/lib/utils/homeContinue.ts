import type { SessionSummary } from "$lib/types/session";
import { formatSessionLabel } from "$lib/utils/formatSession";

export type HomeContinueRow = {
  sessionId: string;
  title: string;
  preview: string;
  relativeTime: string;
};

/** Strip common markdown so Home previews never leak raw `**` / `` ` ``. */
export function stripMarkdownPreview(text: string): string {
  return text
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Lead chat + up to two quieter whispers for Home Continue. */
export function homeContinueRows(
  sessions: SessionSummary[],
  limit = 3,
): HomeContinueRow[] {
  return sessions.slice(0, Math.max(0, limit)).map((session) => {
    const preview = session.preview?.trim() ?? "";
    const firstLine = preview.split("\n")[0]?.trim() ?? "";
    return {
      sessionId: session.session_id,
      title: session.display_name?.trim() || formatSessionLabel(session),
      preview: stripMarkdownPreview(firstLine),
      relativeTime: relativeSessionTime(session.last_timestamp),
    };
  });
}

export function relativeSessionTime(iso?: string | null): string {
  if (!iso) return "";
  try {
    const date = new Date(iso);
    const diffMs = Date.now() - date.getTime();
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 48) return `${hours}h`;
    if (hours < 24 * 7) {
      return date.toLocaleDateString([], { weekday: "short" });
    }
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  } catch {
    return "";
  }
}

export function homeNotesDateParts(now = new Date()): {
  weekday: string;
  day: string;
} {
  return {
    weekday: now.toLocaleDateString([], { weekday: "long" }),
    day: String(now.getDate()),
  };
}

export function peerInitials(label: string | null | undefined): string {
  const parts = (label ?? "")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return `${parts[0][0] ?? ""}${parts[1][0] ?? ""}`.toUpperCase();
}

/** Drop activity whispers that only repeat the status / title. */
export function homeActivityWhisper(
  statusLabel: string,
  title: string,
  line: string,
): string | null {
  const whisper = line.trim();
  if (!whisper) return null;
  const norm = (value: string) =>
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, " ")
      .trim();
  const nWhisper = norm(whisper);
  const nStatus = norm(statusLabel);
  const nTitle = norm(title);
  if (!nWhisper || nWhisper === nStatus || nWhisper === nTitle) return null;
  const tokens = nWhisper.split(" ").filter(Boolean);
  const covered = tokens.every(
    (token) => nStatus.includes(token) || nTitle.includes(token),
  );
  return covered ? null : whisper;
}
