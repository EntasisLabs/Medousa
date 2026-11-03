/**
 * Session list + hydrate/switch/rename/delete/pin. Controllers take a ChatStore
 * host so they never import the store module.
 */

import {
  deriveSession,
  getSessionHistory,
  deleteSession as daemonDeleteSession,
  setSessionDisplayName,
} from "$lib/daemon";
import type { ChatMessage } from "$lib/types/chat";
import type { SessionHistoryResponse } from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import { formatSessionLabel } from "$lib/utils/formatSession";
import {
  applyHistoryToSession,
  applySnapshotToMessages,
  openChatSession,
  projectHistory,
} from "$lib/appChat/client";
import { chatMessageToItem } from "$lib/appChat/fromChatMessage";
import { dedupeMessagesById } from "$lib/utils/mergeTranscript";
import { chatInteractions } from "$lib/liquid/surfaces/chat/chatInteractions";
import { chatStreamPool } from "$lib/chat/chatStreamPool.svelte";
import {
  cloneRuntime,
  emptySessionRuntime,
} from "$lib/chat/chatSessionRuntime";
import { loadDraftForSession } from "$lib/chat/draftPersistence";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { sessionModelSelections } from "./sessionModelSelection.svelte";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import { externalConversationBinding } from "$lib/utils/externalConversationSession";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";
import { mergeSessionSummaries, readMissingPins, readSessionPages } from "./sessionDiscovery";

export const SESSION_KEY = "medousa-home-session-id";
export const PINS_KEY = "medousa-home-pinned-sessions";
const PROMOTED_ASKS_KEY = "medousa-home-promoted-asks-v1";
const SESSIONS_STALE_MS = 30_000;
const SESSIONS_REFRESH_DEBOUNCE_MS = 1_500;
const PROMOTED_ASKS_MAX = 200;
export const TRANSCRIPT_PAGE_SIZE = 24;

export function loadPromotedAskIds(workshopId?: string): Set<string> {
  if (typeof localStorage === "undefined") return new Set();
  try {
    const raw = localStorage.getItem(
      workshopScopedStorageKey(PROMOTED_ASKS_KEY, workshopId),
    );
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return new Set();
    return new Set(
      parsed.filter((entry): entry is string => typeof entry === "string" && entry.trim().length > 0),
    );
  } catch {
    return new Set();
  }
}

export function savePromotedAskIds(ids: Set<string>, workshopId?: string) {
  if (typeof localStorage === "undefined") return;
  const list = [...ids];
  const trimmed =
    list.length > PROMOTED_ASKS_MAX ? list.slice(list.length - PROMOTED_ASKS_MAX) : list;
  localStorage.setItem(
    workshopScopedStorageKey(PROMOTED_ASKS_KEY, workshopId),
    JSON.stringify(trimmed),
  );
}

export function loadPinnedIds(workshopId?: string): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(workshopScopedStorageKey(PINS_KEY, workshopId));
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((id) => typeof id === "string") : [];
  } catch {
    return [];
  }
}

export function loadSessionId(workshopId?: string): string {
  if (typeof localStorage === "undefined") return "";
  const existing = localStorage.getItem(
    workshopScopedStorageKey(SESSION_KEY, workshopId),
  );
  if (existing) return existing;
  return "";
}

/**
 * Project daemon history through the Rust application core.
 * Callers must not reimplement live↔persisted reconciliation in TypeScript.
 */
export async function mapTurns(
  turns: SessionHistoryResponse["turns"],
  options?: {
    lane?: ChatMessage["lane"];
    askJobId?: string | null;
    sessionId?: string;
    authorityId?: string;
  },
): Promise<ChatMessage[]> {
  const sessionId = options?.sessionId?.trim() || "session";
  const authorityId = options?.authorityId?.trim() || "";
  return projectHistory(
    {
      authority_id: authorityId,
      session_id: sessionId,
      turns,
    },
    {
      lane: options?.lane ?? "chat",
      askJobId: options?.askJobId ?? null,
    },
  );
}

async function ensureAppChatSession(sessionId: string, authorityId?: string | null) {
  await openChatSession(sessionId, authorityId ?? "");
}

async function applyCanonicalHistory(
  host: ChatStoreHost,
  sessionId: string,
  history: SessionHistoryResponse,
  options?: { replace?: boolean; lane?: ChatMessage["lane"]; askJobId?: string | null },
): Promise<void> {
  await ensureAppChatSession(sessionId, history.authority_id);
  const snapshot = await applyHistoryToSession({
    sessionId,
    history,
    lane: options?.lane ?? "chat",
    askJobId: options?.askJobId ?? null,
    replace: options?.replace ?? false,
    localItems:
      options?.replace === true
        ? null
        : host.messages.map(chatMessageToItem),
  });
  const applied = applySnapshotToMessages(snapshot);
  host.messages = applied.messages;
  if (applied.activeTurnId) host.activeTurnId = applied.activeTurnId;
  for (const [turnId, seq] of applied.lastSeqByTurn) {
    host.lastSeqByTurn.set(turnId, seq);
  }
}
function historyCursor(history: SessionHistoryResponse): string | null {
  return history.next_cursor?.trim() || null;
}

/**
 * Reconnect from the newest page and keep walking backward only until it
 * overlaps the newest durable entry already rendered. This closes gaps after a
 * long suspension without restoring the old full-transcript download.
 */
async function recentHistoryForMerge(
  sessionId: string,
  localMessages: ChatMessage[],
  shouldContinue: () => boolean,
): Promise<SessionHistoryResponse> {
  const newestLocalSeq = localMessages.reduce((latest, message) => {
    const coordinate = message.transcript;
    return coordinate?.sessionId === sessionId
      ? Math.max(latest, coordinate.entrySeq)
      : latest;
  }, 0);
  const pages: SessionHistoryResponse["turns"][] = [];
  const seenCursors = new Set<string>();
  let cursor: string | undefined;
  let authorityId: SessionHistoryResponse["authority_id"] | undefined;

  while (true) {
    const page = await getSessionHistory(
      sessionId,
      cursor
        ? { limit: TRANSCRIPT_PAGE_SIZE, cursor }
        : { limit: TRANSCRIPT_PAGE_SIZE },
    );
    authorityId = page.authority_id;
    pages.unshift(page.turns);
    if (!shouldContinue()) break;

    const overlapsLocal =
      newestLocalSeq > 0 &&
      page.turns.some((turn) => turn.entry_seq <= newestLocalSeq);
    const nextCursor = historyCursor(page);
    if (
      pages.length >= 3 ||
      newestLocalSeq === 0 ||
      overlapsLocal ||
      !nextCursor ||
      seenCursors.has(nextCursor)
    ) {
      break;
    }
    seenCursors.add(nextCursor);
    cursor = nextCursor;
  }

  const turnsById = new Map<string, SessionHistoryResponse["turns"][number]>();
  for (const turn of pages.flat()) turnsById.set(turn.entry_id, turn);
  const turns = [...turnsById.values()].sort(
    (left, right) => left.entry_seq - right.entry_seq,
  );
  return {
    authority_id: authorityId!,
    session_id: sessionId,
    turns,
  };
}

function derivationIdempotencyKey(): string {
  const suffix = globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  return `home-fork-${suffix}`;
}

/**
 * Materialize committed history through one transcript entry as a new personal
 * session. Draft transfer is deliberately a client composition and never part
 * of the durable context manifest.
 */
export async function forkSessionFromEntry(
  host: ChatStoreHost,
  message: ChatMessage,
  options?: { includeDraft?: boolean },
): Promise<string> {
  const workshopEpoch = host.workshopEpoch;
  const coordinate = message.transcript;
  if (!coordinate?.authorityId || !coordinate.entryId || coordinate.entrySeq < 1) {
    throw new Error("This message is not committed yet");
  }

  const includeDraft = options?.includeDraft === true;
  const sourceDraft =
    includeDraft && host.sessionId.trim() === coordinate.sessionId
      ? host.draft
      : "";
  if (includeDraft && !sourceDraft.trim()) {
    throw new Error("There is no draft to carry into the fork");
  }

  const sourceSession = host.sessions.find(
    (session) => session.session_id === coordinate.sessionId,
  );
  const sourceLabel = sourceSession
    ? formatSessionLabel(sourceSession)
    : coordinate.sessionId === host.sessionId
      ? currentSessionLabel(host)
      : "Conversation";
  const displayName = sourceLabel === "New conversation"
    ? "Forked conversation"
    : `Fork of ${sourceLabel}`;
  const result = await deriveSession(
    {
      intent: "fork",
      sources: [
        {
          session: {
            authority_id: coordinate.authorityId,
            session_id: coordinate.sessionId,
          },
          through_entry_seq: coordinate.entrySeq,
        },
      ],
      target: { catalog: "single", display_name: displayName },
    },
    derivationIdempotencyKey(),
  );
  if (host.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the conversation was being forked");
  }

  await switchSession(host, result.session_id);
  if (host.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the fork was opening");
  }
  if (sourceDraft) {
    host.draft = sourceDraft;
    host.flushDraftPersist();
    host.stashFocusedRuntime();
  }
  if (host.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the fork was opening");
  }
  shellTabs.openChat(result.session_id, {
    activate: true,
    title: result.display_name ?? displayName,
  });
  void refreshSessions(host, { force: true, q: "" });
  return result.session_id;
}

export function isPinned(host: ChatStoreHost, sessionId: string): boolean {
  return host.pinnedIds.includes(sessionId);
}

export function currentSessionLabel(host: ChatStoreHost): string {
  const firstUser = host.messages.find((message) => message.role === "user");
  if (firstUser?.content.trim()) {
    const line = firstUser.content.trim().split("\n")[0];
    return line.length > 48 ? `${line.slice(0, 47)}…` : line;
  }
  const match = host.sessions.find((session) => session.session_id === host.sessionId);
  if (match) return formatSessionLabel(match);
  return "New conversation";
}

export function togglePin(host: ChatStoreHost, sessionId: string) {
  if (host.pinnedIds.includes(sessionId)) {
    host.pinnedIds = host.pinnedIds.filter((id) => id !== sessionId);
  } else {
    host.pinnedIds = [...host.pinnedIds, sessionId];
  }
  localStorage.setItem(
    workshopScopedStorageKey(PINS_KEY, host.workshopScopeId),
    JSON.stringify(host.pinnedIds),
  );
}

export async function renameSession(
  host: ChatStoreHost,
  sessionId: string,
  displayName: string,
): Promise<void> {
  const workshopEpoch = host.workshopEpoch;
  const trimmed = displayName.trim();
  if (!trimmed) {
    throw new Error("Session name must not be empty");
  }
  const response = await setSessionDisplayName(sessionId, trimmed);
  if (host.workshopEpoch !== workshopEpoch) return;
  host.sessions = host.sessions.map((session) =>
    session.session_id === sessionId
      ? { ...session, display_name: response.display_name }
      : session,
  );
}

export async function deleteSession(
  host: ChatStoreHost,
  sessionId: string,
  options?: { purgeMemory?: boolean },
) {
  const workshopEpoch = host.workshopEpoch;
  const trimmed = sessionId.trim();
  if (!trimmed) {
    throw new Error("session_id is required");
  }
  const deletion = await daemonDeleteSession(trimmed, options);
  if (host.workshopEpoch !== workshopEpoch) return;
  const deletionStatus = deletion.status ?? (deletion.deleted ? "complete" : "retryable_partial");
  if (deletionStatus !== "complete" || !deletion.deleted) {
    const failed = (deletion.surfaces ?? [])
      .filter((surface) => !surface.deleted)
      .map((surface) => surface.surface)
      .join(", ");
    throw new Error(
      `Session deletion ${deletionStatus}; retry ${deletion.deletion_id ?? trimmed}${failed ? ` (${failed})` : ""}`,
    );
  }
  sessionModelSelections.clear({ sessionId: trimmed, workshopScopeId: host.workshopScopeId });
  host.sessions = host.sessions.filter((session) => session.session_id !== trimmed);
  host.pinnedIds = host.pinnedIds.filter((id) => id !== trimmed);
  localStorage.setItem(
    workshopScopedStorageKey(PINS_KEY, host.workshopScopeId),
    JSON.stringify(host.pinnedIds),
  );
  if (host.sessionId === trimmed) {
    await host.newSession();
  } else {
    await refreshSessions(host, { force: true });
  }
}

async function fetchSessions(host: ChatStoreHost, _hadCache: boolean, _query = "") {
  const workshopEpoch = host.workshopEpoch;
  const revision = host.sessionsListRevision = (host.sessionsListRevision ?? 0) + 1;
  const current = () => host.workshopEpoch === workshopEpoch && host.sessionsListRevision === revision;
  host.sessionsRefreshing = true;
  host.sessionsLoadingOlder = false;
  host.sessionsError = null;
  try {
    // Refresh the depth already browsed, rather than collapsing back to page one.
    // Search owns a separate result set and never replaces this browse cache.
    const response = await readSessionPages({ pages: host.sessionsBrowsePages ?? 1, current });
    if (!response || !current()) return;
    const previousPins = host.sessions.filter(session => host.pinnedIds.includes(session.session_id));
    host.sessions = response.sessions;
    host.sessionsCursor = response.cursor;
    host.sessionsBrowsePages = response.pages;
    const { pins, failed, failedIds } = await readMissingPins([...host.pinnedIds], response.sessions, current);
    if (!current()) return;
    // A transport failure must not hide a pin we already knew. A confirmed
    // missing/deleted session, in contrast, is not restored from stale cache.
    host.sessions = mergeSessionSummaries(host.sessions,
      previousPins.filter(pin => failedIds.includes(pin.session_id) && host.pinnedIds.includes(pin.session_id)),
      pins.filter(pin => host.pinnedIds.includes(pin.session_id)));
    host.sessionsFetchedAt = Date.now();
    host.sessionsError = failed ? "Some pinned chats couldn’t be loaded. Refresh to retry." : null;
  } catch (err) {
    if (current()) host.sessionsError = err instanceof Error ? err.message : String(err);
  } finally {
    if (current()) host.sessionsRefreshing = false;
  }
}

export async function loadMoreSessions(host: ChatStoreHost) {
  if (host.sessionsRefreshing || host.sessionsLoadingOlder || !host.sessionsCursor) return;
  const epoch = host.workshopEpoch;
  const revision = host.sessionsListRevision ?? 0;
  const current = () => host.workshopEpoch === epoch && (host.sessionsListRevision ?? 0) === revision;
  host.sessionsLoadingOlder = true;
  host.sessionsError = null;
  try {
    const response = await readSessionPages({ pages: 1, cursor: host.sessionsCursor, current });
    if (!response || !current()) return;
    host.sessions = mergeSessionSummaries(host.sessions, response.sessions);
    host.sessionsCursor = response.cursor;
    host.sessionsBrowsePages = (host.sessionsBrowsePages ?? 1) + response.pages;
  } catch (err) {
    if (current()) host.sessionsError = err instanceof Error ? err.message : String(err);
  } finally {
    if (current()) host.sessionsLoadingOlder = false;
  }
}

export async function refreshSessions(
  host: ChatStoreHost,
  options?: { force?: boolean; q?: string },
) {
  const workshopEpoch = host.workshopEpoch;
  const force = options?.force ?? false;
  const query = (options?.q ?? "").trim();
  if (options?.q !== undefined) {
    host.sessionListQuery = query;
  }
  host.sessionsRefreshDesiredQuery = query;

  const hadCache = host.sessions.length > 0;
  const fresh =
    !force && !query && hadCache && Date.now() - host.sessionsFetchedAt < SESSIONS_STALE_MS;

  if (fresh) {
    return;
  }

  if (host.sessionsRefreshInFlight) {
    return host.sessionsRefreshInFlight;
  }

  const inFlight = (async () => {
    while (host.workshopEpoch === workshopEpoch) {
      const q = host.sessionsRefreshDesiredQuery ?? "";
      const cacheHint = host.sessions.length > 0;
      await fetchSessions(host, cacheHint, q);
      if ((host.sessionsRefreshDesiredQuery ?? "") === q) {
        break;
      }
    }
  })();
  host.sessionsRefreshInFlight = inFlight;

  try {
    await inFlight;
  } finally {
    if (host.sessionsRefreshInFlight === inFlight) {
      host.sessionsRefreshInFlight = null;
    }
  }
}

export function scheduleSessionsRefresh(host: ChatStoreHost) {
  if (host.sessionsRefreshTimer) {
    clearTimeout(host.sessionsRefreshTimer);
  }
  host.sessionsRefreshTimer = setTimeout(() => {
    host.sessionsRefreshTimer = null;
    void refreshSessions(host, { force: true });
  }, SESSIONS_REFRESH_DEBOUNCE_MS);
}

export async function warmBackgroundSession(host: ChatStoreHost, sessionId: string) {
  const workshopEpoch = host.workshopEpoch;
  const trimmed = sessionId.trim();
  if (!trimmed || trimmed === host.sessionId || externalConversationBinding(trimmed) || host.history?.isReleased(trimmed)) return;

  const existing = host.sessionRuntimes.get(trimmed);
  if (existing && (existing.messages.length > 0 || existing.historyLoading)) {
    return;
  }

  const runtime = emptySessionRuntime(
    trimmed,
    loadDraftForSession(trimmed, host.workshopScopeId),
  );
  runtime.historyLoading = true;
  host.sessionRuntimes.set(trimmed, runtime);
  host.bumpRuntimeRevision();

  try {
    const history = await getSessionHistory(trimmed, { limit: TRANSCRIPT_PAGE_SIZE });
    if (host.workshopEpoch !== workshopEpoch || host.history?.isReleased(trimmed)) return;
    if (host.sessionId === trimmed) return;
    const current =
      host.sessionRuntimes.get(trimmed) ??
      emptySessionRuntime(trimmed, loadDraftForSession(trimmed, host.workshopScopeId));
    current.messages = await mapTurns(history.turns, {
      sessionId: trimmed,
      authorityId: history.authority_id,
    });
    current.historyCursor = historyCursor(history);
    current.historyLoading = false;
    current.streamError = null;
    host.sessionRuntimes.set(trimmed, current);
    host.messageIndexes.delete(trimmed);
    host.bumpRuntimeRevision();
  } catch (err) {
    if (host.workshopEpoch !== workshopEpoch || host.history?.isReleased(trimmed)) return;
    if (host.sessionId === trimmed) return;
    const current = host.sessionRuntimes.get(trimmed);
    if (!current) return;
    current.historyLoading = false;
    current.streamError = err instanceof Error ? err.message : String(err);
    host.sessionRuntimes.set(trimmed, current);
    host.messageIndexes.delete(trimmed);
    host.bumpRuntimeRevision();
  }
}

export async function newSession(
  host: ChatStoreHost,
  options?: { shellContext?: { desktopId: string; groupId: string } },
) {
  const workshopEpoch = host.workshopEpoch;
  const { createSession } = await import("$lib/daemon");
  const created = await createSession();
  if (host.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the conversation was being created");
  }
  host.flushDraftPersist();
  host.stashFocusedRuntime();
  const id = created.session_id;
  localStorage.setItem(
    workshopScopedStorageKey(SESSION_KEY, host.workshopScopeId),
    id,
  );
  host.loadRuntimeIntoFocused(
    emptySessionRuntime(id, loadDraftForSession(id, host.workshopScopeId)),
  );
  host.sessionPristine = true;
  host.historyLoading = false;
  host.transcriptEpoch += 1;
  chatInteractions.reset();
  host.contextUsage = null;
  host.contextUsagePanelOpen = false;
  chatStreamPool.acquire(id);
  host.stashFocusedRuntime();
  if (host.workshopEpoch !== workshopEpoch) return;
  const shellContext = options?.shellContext;
  if (
    !shellContext ||
    (shellTabs.activeDesktopId === shellContext.desktopId &&
      shellTabs.activeGroupId === shellContext.groupId)
  ) {
    shellTabs.openChat(id, {
      activate: true,
      groupId: shellContext?.groupId,
    });
  }
  void refreshSessions(host, { force: true, q: "" });
  const { workshops } = await import("$lib/stores/workshops.svelte");
  if (host.workshopEpoch !== workshopEpoch) return;
  void workshops.saveActiveSession(id);
}

export async function newSharedRoom(
  host: ChatStoreHost,
  options?: {
    displayName?: string;
    memberProfileIds?: string[];
  },
) {
  const workshopEpoch = host.workshopEpoch;
  const { createSession } = await import("$lib/daemon");
  const { userProfiles } = await import("$lib/stores/userProfiles.svelte");
  const { sharedMode } = await import("$lib/stores/sharedMode.svelte");
  await sharedMode.load();
  if (!sharedMode.isShared) {
    throw new Error("Enable Shared mode in Settings before creating a shared room");
  }
  if (userProfiles.profiles.length === 0) {
    await userProfiles.load({ suppressRemoteNotice: true });
  }
  const members =
    options?.memberProfileIds?.filter((id) => id.trim().length > 0) ??
    userProfiles.profiles.map((profile) => profile.profile_id);
  const created = await createSession({
    catalog: "shared",
    memberProfileIds: members.length > 0 ? members : undefined,
    agentProfileId: sharedMode.generalProfileId,
    displayName: options?.displayName?.trim() || "Shared room",
  });
  if (host.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the shared room was being created");
  }

  host.flushDraftPersist();
  host.stashFocusedRuntime();
  const id = created.session_id;
  localStorage.setItem(
    workshopScopedStorageKey(SESSION_KEY, host.workshopScopeId),
    id,
  );
  host.loadRuntimeIntoFocused(
    emptySessionRuntime(id, loadDraftForSession(id, host.workshopScopeId)),
  );
  host.sessionPristine = true;
  host.historyLoading = false;
  host.transcriptEpoch += 1;
  chatInteractions.reset();
  host.contextUsage = null;
  host.contextUsagePanelOpen = false;
  chatStreamPool.acquire(id);
  host.stashFocusedRuntime();
  await refreshSessions(host, { force: true });
  if (host.workshopEpoch !== workshopEpoch) return;
  shellTabs.openChat(id, { activate: true });
  const { workshops } = await import("$lib/stores/workshops.svelte");
  if (host.workshopEpoch !== workshopEpoch) return;
  void workshops.saveActiveSession(id);
}

export async function ensureSessionHydrated(
  host: ChatStoreHost,
  options?: { notice?: boolean },
) {
  if (host.history?.isReleased(host.sessionId)) return;
  if (!host.sessionId.trim()) {
    if (!host.sessionBootstrapInFlight) {
      const pending = newSession(host);
      const tracked = pending.finally(() => {
        if (host.sessionBootstrapInFlight === tracked) {
          host.sessionBootstrapInFlight = null;
        }
      });
      host.sessionBootstrapInFlight = tracked;
    }
    await host.sessionBootstrapInFlight;
    return;
  }
  if (externalConversationBinding(host.sessionId)) return;
  if (host.historyLoading) return;
  if (host.sessionPristine) return;
  if (host.messages.length === 0) {
    await reloadCurrentSession(host, options);
    return;
  }
  await reconcileOnResume(host, { notice: options?.notice });
  host.sanitizeTranscript();
}

export async function reconcileOnResume(
  host: ChatStoreHost,
  options?: { notice?: boolean },
  cards: WorkCard[] = [],
) {
  const workshopEpoch = host.workshopEpoch;
  const sessionId = host.sessionId.trim();
  if (!sessionId || externalConversationBinding(sessionId)) return;

  const epoch = host.transcriptEpoch;
  const stillSameSession = () =>
    workshopEpoch === host.workshopEpoch &&
    epoch === host.transcriptEpoch &&
    host.sessionId.trim() === sessionId && !host.history?.isReleased(sessionId);

  try {
    await host.tryReattachActiveTurn(cards);
    if (!stillSameSession()) return;

    // Reattaching a live ticket and reconciling committed history are separate
    // responsibilities. A daemon may commit one turn while another ticket is
    // still active, especially after a mobile client was suspended. Always
    // merge the durable projection so reconnecting clients see those commits;
    // the Rust app core preserves the live placeholder for the attached turn.
    const history = await recentHistoryForMerge(
      sessionId,
      host.messages,
      stillSameSession,
    );
    if (!stillSameSession()) return;

    await applyCanonicalHistory(host, sessionId, history, { replace: false });
    if (!stillSameSession()) return;
    host.sanitizeTranscript();
  } catch (err) {
    host.noteResumeFailure(err);
  }
}

export async function reloadCurrentSession(
  host: ChatStoreHost,
  options?: { notice?: boolean },
) {
  const workshopEpoch = host.workshopEpoch;
  const sessionId = host.sessionId.trim();
  if (!sessionId || externalConversationBinding(sessionId)) return;

  const epoch = host.transcriptEpoch;
  const stillSameSession = () =>
    workshopEpoch === host.workshopEpoch &&
    epoch === host.transcriptEpoch &&
    host.sessionId.trim() === sessionId && !host.history?.isReleased(sessionId);

  host.historyLoading = true;
  host.historyLoadingOlder = false;
  host.streamError = null;
  try {
    const history = await getSessionHistory(sessionId, { limit: TRANSCRIPT_PAGE_SIZE });
    if (!stillSameSession()) return;
    await applyCanonicalHistory(host, sessionId, history, { replace: false });
    if (!stillSameSession()) return;
    host.historyCursor = historyCursor(history);
    if (options?.notice !== false && history.turns.length > 0) {
      const count = history.turns.length;
      host.historyNotice = `Restored ${count} turn${count === 1 ? "" : "s"}`;
    }
    await host.tryReattachActiveTurn();
  } catch (err) {
    if (stillSameSession()) {
      host.streamError = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (stillSameSession()) {
      host.historyLoading = false;
    }
  }
}

export async function loadOlderHistory(host: ChatStoreHost, sessionId: string): Promise<number> {
  return host.history?.loadOlder(sessionId.trim()) ?? 0;
}

export async function switchSession(host: ChatStoreHost, sessionId: string) {
  const workshopEpoch = host.workshopEpoch;
  const sourceSessionId = host.sessionId.trim();
  const mirrorShellChat = () => {
    chatStreamPool.acquire(sessionId);
    void Promise.resolve().then(() => {
      if (host.workshopEpoch !== workshopEpoch) return;
      if (host.sessionId.trim() !== sessionId) return;
      const active = shellTabs.activeTab;
      if (active?.kind !== "chat" || active.sessionId !== sourceSessionId) return;
      if (active.sessionId === sessionId) return;
      shellTabs.openChat(sessionId, {
        activate: true,
        groupId: shellTabs.activeGroupId,
      });
    });
  };

  const trimmed = sessionId.trim();
  if (!trimmed) return;
  host.history?.open(trimmed);

  if (trimmed === host.sessionId) {
    await reloadCurrentSession(host, { notice: false });
    if (host.workshopEpoch !== workshopEpoch) return;
    host.stashFocusedRuntime();
    mirrorShellChat();
    return;
  }

  host.flushDraftPersist();
  host.stashFocusedRuntime();
  host.transcriptEpoch += 1;
  const switchEpoch = host.transcriptEpoch;

  const cached = host.sessionRuntimes.get(trimmed);
  if (cached && cached.messages.length > 0) {
    const runtime = cloneRuntime(cached);
    runtime.transcriptEpoch = switchEpoch;
    host.loadRuntimeIntoFocused(runtime);
    host.transcriptEpoch = switchEpoch;
    localStorage.setItem(
      workshopScopedStorageKey(SESSION_KEY, host.workshopScopeId),
      trimmed,
    );
    // Retained scenes and stream cursors must survive session switches together.
    chatInteractions.reset();
    chatStreamPool.acquire(trimmed);
    host.stashFocusedRuntime();
    const { workshops } = await import("$lib/stores/workshops.svelte");
    if (host.workshopEpoch !== workshopEpoch) return;
    void workshops.saveActiveSession(trimmed);
    void reconcileOnResume(host, { notice: false });
    mirrorShellChat();
    return;
  }

  const fresh = emptySessionRuntime(
    trimmed,
    loadDraftForSession(trimmed, host.workshopScopeId),
  );
  fresh.historyLoading = true;
  fresh.transcriptEpoch = switchEpoch;
  host.loadRuntimeIntoFocused(fresh);
  host.transcriptEpoch = switchEpoch;
  localStorage.setItem(
    workshopScopedStorageKey(SESSION_KEY, host.workshopScopeId),
    trimmed,
  );
  chatInteractions.reset();
  if (externalConversationBinding(trimmed)) {
    host.historyLoading = false;
    host.stashFocusedRuntime();
    mirrorShellChat();
    const { workshops } = await import("$lib/stores/workshops.svelte");
    if (host.workshopEpoch === workshopEpoch && host.sessionId === trimmed) void workshops.saveActiveSession(trimmed);
    return;
  }
  try {
    const history = await getSessionHistory(trimmed, { limit: TRANSCRIPT_PAGE_SIZE });
    if (
      host.workshopEpoch !== workshopEpoch ||
      host.sessionId !== trimmed ||
      switchEpoch !== host.transcriptEpoch
    ) return;
    await applyCanonicalHistory(host, trimmed, history, { replace: true });
    if (
      host.workshopEpoch !== workshopEpoch ||
      host.sessionId !== trimmed ||
      switchEpoch !== host.transcriptEpoch
    ) return;
    host.historyCursor = historyCursor(history);
    const { workshops } = await import("$lib/stores/workshops.svelte");
    void workshops.saveActiveSession(trimmed);
  } catch (err) {
    if (
      host.workshopEpoch === workshopEpoch &&
      host.sessionId === trimmed &&
      switchEpoch === host.transcriptEpoch
    ) {
      host.streamError = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (
      host.workshopEpoch === workshopEpoch &&
      host.sessionId === trimmed &&
      switchEpoch === host.transcriptEpoch
    ) {
      host.historyLoading = false;
    }
  }
  if (host.workshopEpoch !== workshopEpoch || host.sessionId !== trimmed ||
    switchEpoch !== host.transcriptEpoch || host.history?.isReleased(trimmed)) return;
  host.stashFocusedRuntime();
  chatStreamPool.acquire(trimmed);
  void host.tryReattachActiveTurn();
  mirrorShellChat();
}

