/**
 * Session list + hydrate/switch/rename/delete/pin. Controllers take a ChatStore
 * host so they never import the store module.
 */

import {
  deriveSession,
  getSessionHistory,
  listSessions,
  deleteSession as daemonDeleteSession,
  setSessionDisplayName,
} from "$lib/daemon";
import type { ChatMessage } from "$lib/types/chat";
import type { SessionHistoryResponse } from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import { isAskJobId, askSessionId } from "$lib/types/askJob";
import {
  chatSegmentsFromParts,
  hostContextFromParts,
  modelReceiptFromParts,
  reasoningFromParts,
  progressFromParts,
  toolRunsFromParts,
  userMediaFromParts,
  uiArtifactsFromParts,
} from "$lib/types/turnParts";
import { formatSessionLabel } from "$lib/utils/formatSession";
import { dedupeMessagesById, mergeTranscript } from "$lib/utils/mergeTranscript";
import { chatScenes } from "$lib/liquid/surfaces/chat/chatScenes.svelte";
import { chatInteractions } from "$lib/liquid/surfaces/chat/chatInteractions";
import { chatStreamPool } from "$lib/chat/chatStreamPool.svelte";
import {
  cloneRuntime,
  emptySessionRuntime,
} from "$lib/chat/chatSessionRuntime";
import { loadDraftForSession } from "$lib/chat/draftPersistence";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import { workshopScopedStorageKey } from "$lib/utils/workshopLocality";

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

function normalizeRole(role: string): ChatMessage["role"] {
  if (role === "user" || role === "assistant" || role === "system") {
    return role;
  }
  return "assistant";
}

export function mapTurns(
  turns: SessionHistoryResponse["turns"],
  options?: {
    lane?: ChatMessage["lane"];
    askJobId?: string | null;
    sessionId?: string;
    authorityId?: string;
  },
): ChatMessage[] {
  const lane = options?.lane ?? "chat";
  const askJobId = options?.askJobId ?? null;
  const sessionId = options?.sessionId?.trim() || "session";
  const authorityId = options?.authorityId?.trim() || "";
  return turns.map((turn, index) => {
    const modelReceipt = modelReceiptFromParts(turn.parts ?? null);
    const entryId = turn.entry_id?.trim();
    const segments =
      turn.role === "assistant" ? chatSegmentsFromParts(turn.parts ?? null) : undefined;
    return {
      id: entryId
        ? `${sessionId}:${entryId}`
        : `${sessionId}:${turn.timestamp}:${turn.role}:${index}`,
      role: normalizeRole(turn.role),
      content: turn.content,
      lane,
      askJobId,
      turnIndex: turn.entry_seq || index + 1,
      answerState: turn.answer_state ?? null,
      tools: turn.tool_names?.length ? turn.tool_names : undefined,
      toolRuns: toolRunsFromParts(turn.parts ?? null),
      segments,
      uiArtifacts: uiArtifactsFromParts(turn.parts ?? null),
      reasoning: reasoningFromParts(turn.parts ?? null),
      statusLine:
        turn.role === "assistant" && segments === undefined
          ? progressFromParts(turn.parts ?? null)
          : null,
      mediaAttachments: userMediaFromParts(turn.parts ?? null),
      hostContext: hostContextFromParts(turn.parts ?? null),
      speakerProfileId: turn.speaker_profile_id?.trim() || null,
      responseProvider: modelReceipt?.provider ?? null,
      responseModel: modelReceipt?.model ?? null,
      transcript:
        authorityId && turn.entry_id && turn.entry_seq
          ? {
              authorityId,
              sessionId,
              entryId: turn.entry_id,
              entrySeq: turn.entry_seq,
              source: turn.source
                ? {
                    authorityId: turn.source.session.authority_id,
                    sessionId: turn.source.session.session_id,
                    entryId: turn.source.entry_id,
                    entrySeq: turn.source.entry_seq,
                  }
                : null,
            }
          : null,
    };
  });
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
  const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
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

async function fetchSessions(host: ChatStoreHost, hadCache: boolean, query = "") {
  const workshopEpoch = host.workshopEpoch;
  host.sessionsRefreshing = hadCache;
  if (!hadCache) {
    host.sessionsError = null;
  }
  try {
    const response = await listSessions({
      limit: 50,
      includeVerification: false,
      q: query || undefined,
    });
    if (host.workshopEpoch !== workshopEpoch) return;
    host.sessions = response.sessions;
    host.sessionsFetchedAt = Date.now();
    host.sessionsError = null;
  } catch (err) {
    if (host.workshopEpoch !== workshopEpoch) return;
    if (!hadCache) {
      host.sessionsError = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (host.workshopEpoch === workshopEpoch) {
      host.sessionsRefreshing = false;
    }
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
  if (!trimmed || trimmed === host.sessionId) return;

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
    if (host.workshopEpoch !== workshopEpoch) return;
    if (host.sessionId === trimmed) return;
    const current =
      host.sessionRuntimes.get(trimmed) ??
      emptySessionRuntime(trimmed, loadDraftForSession(trimmed, host.workshopScopeId));
    current.messages = mapTurns(history.turns, {
      sessionId: trimmed,
      authorityId: history.authority_id,
    });
    current.historyCursor = historyCursor(history);
    current.historyLoading = false;
    current.streamError = null;
    host.sessionRuntimes.set(trimmed, current);
    host.bumpRuntimeRevision();
  } catch (err) {
    if (host.workshopEpoch !== workshopEpoch) return;
    if (host.sessionId === trimmed) return;
    const current = host.sessionRuntimes.get(trimmed);
    if (!current) return;
    current.historyLoading = false;
    current.streamError = err instanceof Error ? err.message : String(err);
    host.sessionRuntimes.set(trimmed, current);
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
  chatScenes.reset();
  chatInteractions.reset();
  host.contextUsage = null;
  host.contextUsagePanelOpen = false;
  chatStreamPool.acquire(id);
  host.stashFocusedRuntime();
  const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
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
  chatScenes.reset();
  chatInteractions.reset();
  host.contextUsage = null;
  host.contextUsagePanelOpen = false;
  chatStreamPool.acquire(id);
  host.stashFocusedRuntime();
  await refreshSessions(host, { force: true });
  if (host.workshopEpoch !== workshopEpoch) return;
  const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
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
  if (!sessionId) return;

  const epoch = host.transcriptEpoch;
  const stillSameSession = () =>
    workshopEpoch === host.workshopEpoch &&
    epoch === host.transcriptEpoch &&
    host.sessionId.trim() === sessionId;

  try {
    const attached = await host.tryReattachActiveTurn(cards);
    if (!stillSameSession()) return;

    const liveStream =
      attached &&
      (host.messages.some(
        (message) =>
          message.streaming &&
          message.lane !== "worker" &&
          message.phase !== "budget_blocked",
      ) ||
        hasLiveInteractiveTurn(host));

    if (liveStream) {
      host.sanitizeTranscript();
      return;
    }

    const history = await recentHistoryForMerge(
      sessionId,
      host.messages,
      stillSameSession,
    );
    if (!stillSameSession()) return;

    const daemonMessages = mapTurns(history.turns, {
      sessionId,
      authorityId: history.authority_id,
    });
    host.messages = mergeTranscript(host.messages, daemonMessages);
    host.sanitizeTranscript();
  } catch (err) {
    host.noteResumeFailure(err);
  }
}

function hasLiveInteractiveTurn(host: ChatStoreHost): boolean {
  for (const turn of host.turns.values()) {
    if (turn.mode !== "interactive" || turn.terminal) continue;
    if (host.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
    return true;
  }
  return false;
}

export async function reloadCurrentSession(
  host: ChatStoreHost,
  options?: { notice?: boolean },
) {
  const workshopEpoch = host.workshopEpoch;
  const sessionId = host.sessionId.trim();
  if (!sessionId) return;

  const epoch = host.transcriptEpoch;
  const stillSameSession = () =>
    workshopEpoch === host.workshopEpoch &&
    epoch === host.transcriptEpoch &&
    host.sessionId.trim() === sessionId;

  host.historyLoading = true;
  host.historyLoadingOlder = false;
  host.streamError = null;
  try {
    const history = await getSessionHistory(sessionId, { limit: TRANSCRIPT_PAGE_SIZE });
    if (!stillSameSession()) return;
    host.messages = mapTurns(history.turns, {
      sessionId,
      authorityId: history.authority_id,
    });
    host.historyCursor = historyCursor(history);
    if (options?.notice !== false && history.turns.length > 0) {
      const count = history.turns.length;
      host.historyNotice = `Restored ${count} turn${count === 1 ? "" : "s"}`;
    }
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

export async function loadOlderHistory(
  host: ChatStoreHost,
  sessionId: string,
): Promise<number> {
  const workshopEpoch = host.workshopEpoch;
  const trimmed = sessionId.trim();
  const cursor = host.historyCursor?.trim();
  if (
    !trimmed ||
    trimmed !== host.sessionId.trim() ||
    !cursor ||
    host.historyLoadingOlder
  ) {
    return 0;
  }

  const epoch = host.transcriptEpoch;
  const stillSameSession = () =>
    workshopEpoch === host.workshopEpoch &&
    epoch === host.transcriptEpoch &&
    host.sessionId.trim() === trimmed;

  host.historyLoadingOlder = true;
  try {
    const history = await getSessionHistory(trimmed, {
      limit: TRANSCRIPT_PAGE_SIZE,
      cursor,
    });
    if (!stillSameSession()) return 0;

    const older = mapTurns(history.turns, {
      sessionId: trimmed,
      authorityId: history.authority_id,
    });
    host.messages = dedupeMessagesById([...older, ...host.messages]);
    const nextCursor = historyCursor(history);
    host.historyCursor = nextCursor === cursor ? null : nextCursor;
    host.sanitizeTranscript();
    return older.length;
  } finally {
    if (workshopEpoch === host.workshopEpoch) {
      host.withSessionFields(trimmed, () => {
        if (host.transcriptEpoch === epoch) host.historyLoadingOlder = false;
      });
    }
  }
}

export async function switchSession(host: ChatStoreHost, sessionId: string) {
  const workshopEpoch = host.workshopEpoch;
  const sourceSessionId = host.sessionId.trim();
  const mirrorShellChat = () => {
    chatStreamPool.acquire(sessionId);
    void import("$lib/stores/shellTabs.svelte").then(({ shellTabs }) => {
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
    chatScenes.reset();
    chatInteractions.reset();
    chatStreamPool.acquire(trimmed);
    host.stashFocusedRuntime();
    const { workshops } = await import("$lib/stores/workshops.svelte");
    if (host.workshopEpoch !== workshopEpoch) return;
    void workshops.saveActiveSession(trimmed);
    void host.tryReattachActiveTurn();
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
  chatScenes.reset();
  chatInteractions.reset();
  try {
    const history = await getSessionHistory(trimmed, { limit: TRANSCRIPT_PAGE_SIZE });
    if (
      host.workshopEpoch !== workshopEpoch ||
      host.sessionId !== trimmed ||
      switchEpoch !== host.transcriptEpoch
    ) return;
    host.messages = mapTurns(history.turns, {
      sessionId: trimmed,
      authorityId: history.authority_id,
    });
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
  if (host.workshopEpoch !== workshopEpoch) return;
  host.stashFocusedRuntime();
  chatStreamPool.acquire(trimmed);
  void host.tryReattachActiveTurn();
  mirrorShellChat();
}

export function promoteAskToChat(host: ChatStoreHost, jobId: string) {
  const trimmed = jobId.trim();
  if (!trimmed) return;
  host.messages = dedupeMessagesById(
    host.messages.map((message) =>
      message.askJobId === trimmed ? { ...message, lane: "chat", askJobId: null } : message,
    ),
  );
  host.promotedAskIds.add(trimmed);
  savePromotedAskIds(host.promotedAskIds, host.workshopScopeId);
}

export async function hydrateAskThreads(host: ChatStoreHost, cards: WorkCard[]) {
  const workshopEpoch = host.workshopEpoch;
  const epoch = host.transcriptEpoch;
  const targets = cards.filter((card) => {
    if (!isAskJobId(card.id)) return false;
    if (host.promotedAskIds.has(card.id)) return false;
    if (host.askHydrationInFlight.has(card.id)) return false;
    return !host.messages.some((message) => message.askJobId === card.id);
  });
  if (targets.length === 0) return;

  for (const card of targets) {
    host.askHydrationInFlight.add(card.id);
  }

  try {
    const batches = await Promise.all(
      targets.map(async (card) => {
        try {
          const sessionId = askSessionId(card.id);
          const history = await getSessionHistory(sessionId, {
            limit: TRANSCRIPT_PAGE_SIZE,
          });
          if (
            workshopEpoch !== host.workshopEpoch ||
            epoch !== host.transcriptEpoch ||
            history.turns.length === 0
          ) {
            return [] as ChatMessage[];
          }
          return mapTurns(history.turns, {
            lane: "ask",
            askJobId: card.id,
            sessionId,
            authorityId: history.authority_id,
          });
        } catch {
          return [] as ChatMessage[];
        }
      }),
    );

    if (workshopEpoch !== host.workshopEpoch || epoch !== host.transcriptEpoch) return;

    const hydrated = batches.flat();
    if (hydrated.length === 0) return;

    const jobsAlreadyHydrated = new Set(
      host.messages
        .map((message) => message.askJobId)
        .filter((jobId): jobId is string => Boolean(jobId?.trim())),
    );
    const fresh = hydrated.filter((message) => {
      const jobId = message.askJobId?.trim();
      if (!jobId) return true;
      if (host.promotedAskIds.has(jobId)) return false;
      if (jobsAlreadyHydrated.has(jobId)) return false;
      return true;
    });
    if (fresh.length === 0) return;

    host.messages = dedupeMessagesById([...host.messages, ...fresh]);
  } finally {
    for (const card of targets) {
      host.askHydrationInFlight.delete(card.id);
    }
  }
}
