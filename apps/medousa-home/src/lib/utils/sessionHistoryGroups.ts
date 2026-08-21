import type { SessionSummary } from "$lib/types/session";

export type SessionHistoryGroupId = "today" | "yesterday" | "week" | "older";

export interface SessionHistoryGroup {
  id: SessionHistoryGroupId;
  label: string;
  sessions: SessionSummary[];
}

const GROUPS: Array<{ id: SessionHistoryGroupId; label: string }> = [
  { id: "today", label: "Today" },
  { id: "yesterday", label: "Yesterday" },
  { id: "week", label: "Previous 7 days" },
  { id: "older", label: "Older" },
];

export function groupSessionsByRecency(
  sessions: SessionSummary[],
  now = new Date(),
): SessionHistoryGroup[] {
  const buckets = new Map<SessionHistoryGroupId, SessionSummary[]>(
    GROUPS.map((group) => [group.id, []]),
  );
  const today = startOfLocalDay(now).getTime();

  for (const session of sessions) {
    const timestamp = session.last_timestamp
      ? new Date(session.last_timestamp).getTime()
      : Number.NaN;
    const day = Number.isFinite(timestamp)
      ? startOfLocalDay(new Date(timestamp)).getTime()
      : Number.NEGATIVE_INFINITY;
    const daysAgo = Math.round((today - day) / 86_400_000);
    const id: SessionHistoryGroupId =
      daysAgo <= 0
        ? "today"
        : daysAgo === 1
          ? "yesterday"
          : daysAgo <= 7
            ? "week"
            : "older";
    buckets.get(id)?.push(session);
  }

  return GROUPS.flatMap((group) => {
    const grouped = buckets.get(group.id) ?? [];
    return grouped.length > 0 ? [{ ...group, sessions: grouped }] : [];
  });
}

function startOfLocalDay(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate());
}
