import { listSessions } from "$lib/daemon/session";
import type { SessionSummary } from "$lib/types/session";
import type { ChatStoreHost } from "./chatStoreHost";

/** Invalidate catalog work and clear workshop-scoped browse state together. */
export function resetSessionCatalog(host: ChatStoreHost) {
  if (host.sessionsRefreshTimer) clearTimeout(host.sessionsRefreshTimer);
  host.sessionsRefreshTimer = null;
  host.sessionsRefreshInFlight = null;
  host.sessionBootstrapInFlight = null;
  host.sessionsRefreshDesiredQuery = null;
  host.sessions = [];
  host.sessionsCursor = null;
  host.sessionsLoadingOlder = false;
  host.sessionsBrowsePages = 1;
  host.sessionsListRevision = (host.sessionsListRevision ?? 0) + 1;
  host.sessionsFetchedAt = 0;
  host.sessionListQuery = "";
  host.sessionsError = null;
  host.sessionsRefreshing = false;
  host.pinnedIds = [];
}

export const SESSION_LIST_PAGE_SIZE = 50;

export function mergeSessionSummaries(...lists: SessionSummary[][]): SessionSummary[] {
  const rows = new Map<string, SessionSummary>();
  for (const list of lists) for (const session of list) rows.set(session.session_id, session);
  return [...rows.values()];
}

/** Search walks matching pages, not the recent-chat cache. No transcripts are read. */
export async function readSessionPages(options: {
  q?: string;
  pages?: number;
  cursor?: string;
  current: () => boolean;
}) {
  const rows: SessionSummary[] = [];
  const seen = new Set<string>();
  let cursor = options.cursor;
  let pages = 0;
  while (options.current()) {
    if (cursor) seen.add(cursor);
    const response = await listSessions({ limit: SESSION_LIST_PAGE_SIZE, includeVerification: false, q: options.q, cursor });
    if (!options.current()) return null;
    rows.push(...response.sessions);
    pages += 1;
    cursor = response.next_cursor?.trim() || undefined;
    if (cursor && seen.has(cursor)) throw new Error("The session list returned a repeated page. Try again.");
    if (!cursor || pages >= (options.pages ?? Infinity)) break;
  }
  return options.current() ? { sessions: mergeSessionSummaries(rows), cursor: cursor ?? null, pages } : null;
}

/** Exact IDs only: a substring hit must never masquerade as a saved pin. */
export async function readMissingPins(ids: string[], sessions: SessionSummary[], current: () => boolean) {
  const known = new Set(sessions.map(session => session.session_id));
  const queue = [...new Set(ids)].filter(id => !known.has(id));
  const pins: SessionSummary[] = [];
  const failures: string[] = [];
  async function lane() {
    while (current() && queue.length) {
      const id = queue.shift()!;
      try {
        const response = await readSessionPages({ q: id, current });
        const pin = response?.sessions.find(session => session.session_id === id);
        if (pin && current()) pins.push(pin);
      } catch { if (current()) failures.push(id); }
    }
  }
  await Promise.all([lane(), lane(), lane()]);
  return { pins, failed: failures.length > 0, failedIds: failures };
}
