import type { SessionSummary } from "$lib/types/session";
import type { RepositoryCatalogEntry } from "$lib/forge";
import { formatSessionLabel } from "$lib/utils/formatSession";

export type HomeContinueRow = {
  sessionId: string;
  title: string;
  preview: string;
  relativeTime: string;
};

export type HomeProjectRow = {
  path: string;
  title: string;
  preview: string;
  relativeTime: string;
  /** Open this undertaking when present; otherwise land on Code explorer. */
  workId: string | null;
  workTitle: string | null;
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
  const rows: HomeContinueRow[] = [];
  const seenTitles = new Set<string>();

  for (const session of sessions) {
    if (rows.length >= Math.max(0, limit)) break;

    const title = stripMarkdownPreview(
      session.display_name?.trim() || formatSessionLabel(session),
    );
    if (!isUsefulContinueTitle(title)) continue;

    const titleKey = normalizeContinueText(title);
    if (seenTitles.has(titleKey)) continue;
    seenTitles.add(titleKey);

    const preview = session.preview?.trim() ?? "";
    const firstLine = preview.split("\n")[0]?.trim() ?? "";
    const cleanPreview = stripMarkdownPreview(firstLine);
    rows.push({
      sessionId: session.session_id,
      title,
      preview:
        normalizeContinueText(cleanPreview) === titleKey ? "" : cleanPreview,
      relativeTime: relativeSessionTime(session.last_timestamp),
    });
  }

  return rows;
}

const CANCELLED_TITLE_SUFFIX =
  /(?:\s*\((?:cancelled|canceled)\)|\s+[-–—:]\s*(?:cancelled|canceled))\s*$/i;
const LOW_SIGNAL_TITLE = /^(?:new conversation|new chat|untitled|test(?:ing)?|h(?:i+|e+y+)|hello+|yo+)(?:\s+\d+)?$/i;

function isUsefulContinueTitle(title: string): boolean {
  const trimmed = title.trim();
  if (!trimmed || CANCELLED_TITLE_SUFFIX.test(trimmed)) return false;
  return !LOW_SIGNAL_TITLE.test(normalizeContinueText(trimmed));
}

function normalizeContinueText(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

const ACTIVE_PROJECT_PHASES = new Set(["work", "prepare", "review"]);

function pickProjectWork(entry: RepositoryCatalogEntry): {
  id: string;
  title: string;
} | null {
  const projects = entry.existing_projects ?? [];
  if (projects.length === 0) return null;
  const active = projects.find((project) =>
    ACTIVE_PROJECT_PHASES.has(project.human_phase),
  );
  const pick = active ?? projects[0];
  return { id: pick.id, title: pick.title };
}

/** Recent code repositories for Home — pinned first, then last used. */
export function homeProjectRows(
  entries: RepositoryCatalogEntry[],
  limit = 3,
): HomeProjectRow[] {
  return entries
    .filter((entry) => entry.available && !entry.archived)
    .slice()
    .sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      const aTs = Date.parse(a.last_used_at) || 0;
      const bTs = Date.parse(b.last_used_at) || 0;
      return bTs - aTs;
    })
    .slice(0, Math.max(0, limit))
    .map((entry) => {
      const work = pickProjectWork(entry);
      const branch =
        entry.current_branch?.trim() ||
        entry.suggested_base_ref?.trim() ||
        "";
      const preview =
        work?.title?.trim() ||
        (entry.dirty ? "Recently edited" : branch ? "Open project" : "Project");
      return {
        path: entry.path,
        title: entry.display_name?.trim() || entry.path,
        preview,
        relativeTime: relativeSessionTime(entry.last_used_at),
        workId: work?.id ?? null,
        workTitle: work?.title ?? null,
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
