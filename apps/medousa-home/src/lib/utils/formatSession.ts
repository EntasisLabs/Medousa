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
    if (firstLine) return truncate(firstLine, 48);
  }

  if (looksLikeId(session.session_id)) return "New conversation";
  return session.session_id;
}

function looksLikeId(value: string): boolean {
  return UUID_LIKE.test(value) || /^sess[_-]/i.test(value);
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1)}…`;
}
