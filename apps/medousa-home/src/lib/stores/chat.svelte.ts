import {
  cancelActiveSessionTurn,
  getActiveSessionTurn,
  getSessionHistory,
  listSessionTurns,
  listSessions,
  deleteSession as daemonDeleteSession,
  setSessionDisplayName,
  startInteractiveStream,
  stopInteractiveStreamTurn,
} from "$lib/daemon";
import type {
  ChatMessage,
  InteractiveTurnStreamEvent,
  PendingBudgetApproval,
  ToolRunState,
  TurnTicketState,
} from "$lib/types/chat";
import type { WorkCardDetail } from "$lib/types/card";
import type {
  SessionHistoryResponse,
  SessionSummary,
  TurnTicketRecord,
  TurnTicketResponse,
} from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import { isAskJobId, askJobIdFromSession, askSessionId } from "$lib/types/askJob";
import { reasoningFromParts, progressFromParts, toolRunsFromParts, userMediaFromParts, uiArtifactsFromParts } from "$lib/types/turnParts";
import { mapStreamUiArtifact, replaceUiArtifactEntry } from "$lib/types/artifact";
import type { MediaRef } from "$lib/types/media";
import { chatMediaAttachmentsFromRefs } from "$lib/utils/chatMediaUpload";
import { formatSessionLabel } from "$lib/utils/formatSession";
import {
  isEngineTelemetryText,
  operatorStreamErrorLine,
  operatorStreamStatusLine,
  shouldSuppressStreamContentDelta,
} from "$lib/utils/chatStreamDisplay";
import { dedupeMessagesById, mergeTranscript } from "$lib/utils/mergeTranscript";
import {
  shouldAcceptStreamEvent,
  shouldReattachTurnRecord,
  type StreamOwner,
} from "$lib/utils/streamOwnership";
import { resolveTurnContent } from "$lib/utils/resolveTurnContent";
import { friendlyUserError, MAX_MEDIA_REFS_PER_TURN } from "$lib/utils/normieErrors";
import { settings } from "$lib/stores/settings.svelte";
import {
  isBudgetApprovalStreamEvent,
  isTerminalContentCommit,
  isWorkerHandoffStreamEvent,
} from "$lib/utils/streamEvents";
import { workerStatusLineForColumn } from "$lib/utils/workerThreads";
import { budgetRequestIdFromStreamEvent } from "$lib/notifications";
import type { VaultNoteContextScope } from "$lib/utils/vaultNoteBridge";
import type { ScriptWorkbenchContextScope } from "$lib/utils/scriptWorkbenchBridge";

const SESSION_KEY = "medousa-home-session-id";
const PINS_KEY = "medousa-home-pinned-sessions";
const SESSIONS_STALE_MS = 30_000;
const SESSIONS_REFRESH_DEBOUNCE_MS = 1_500;

interface WorkerLink {
  workId: string;
  parentTurnId: string | null;
  /** Handoff ack bubble ("let me see…"). */
  messageId: string | null;
  /** Follow-up bubble for worker synthesis. */
  synthesisMessageId: string | null;
  sessionId: string;
  synthesisDelivered: boolean;
}

export class ChatStore {
  private askHydrationInFlight = new Set<string>();

  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state("");
  /** Vault note scope when chat opened from Library (Phase D3). */
  vaultNoteContext = $state<VaultNoteContextScope | null>(null);
  pinVaultNoteContext = $state(false);
  /** Grapheme script scope when chat opened from Automations workbench (W6.4). */
  scriptWorkbenchContext = $state<ScriptWorkbenchContextScope | null>(null);
  pinScriptWorkbenchContext = $state(false);
  /** Files uploaded to local daemon, waiting to send with the next turn. */
  pendingMediaRefs = $state<MediaRef[]>([]);
  pendingMediaUploading = $state(false);
  /** Worker handoffs and operator pauses still running outside the live stream. */
  backgroundActivity = $state(0);
  streamError = $state<string | null>(null);
  sessions = $state<SessionSummary[]>([]);
  sessionListQuery = $state("");
  sessionsError = $state<string | null>(null);
  /** True while revalidating the session list without clearing cached rows. */
  sessionsRefreshing = $state(false);
  pinnedIds = $state<string[]>(loadPinnedIds());
  historyLoading = $state(false);
  /** Skip daemon history fetch until the first turn is sent (newSession). */
  sessionPristine = $state(false);
  /** Brief banner after reloading turns from the engine (e.g. after WebView refresh). */
  historyNotice = $state<string | null>(null);
  /** Desktop in-app alert when a turn pauses for budget approval. */
  budgetAlert = $state<PendingBudgetApproval | null>(null);
  /** Daemon turn id for the live interactive stream, if any. */
  activeTurnId = $state<string | null>(null);
  /** Turn-centric state keyed by daemon turn id. */
  turns = $state<Map<string, TurnTicketState>>(new Map());
  /** Turn worker cards linked to chat handoff bubbles (Tier 3). */
  workers = $state<Map<string, WorkerLink>>(new Map());
  private assistantId: string | null = null;
  /** Bumps when the local transcript changes; stale daemon reloads must not overwrite it. */
  private transcriptEpoch = 0;
  private sessionsFetchedAt = 0;
  private sessionsRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private sessionsRefreshInFlight: Promise<void> | null = null;
  /** Turn ids with an active local SSE listener (subset of `turns`). */
  private streamOwners = new Map<string, StreamOwner>();

  /** True while the composer must wait — Tier 2c: always open. */
  get composerBlocked(): boolean {
    return false;
  }

  /** Interactive turn still streaming tokens (UI pulse only). */
  get liveStreamActive(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (this.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
      return true;
    }
    return false;
  }

  /** Non-terminal interactive ticket in flight (fork policy gate). */
  hasLiveInteractiveTurn(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (this.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
      return true;
    }
    return false;
  }

  /** Live stream and/or background worker / approval work in flight. */
  get hasTurnActivity(): boolean {
    return this.liveStreamActive || this.backgroundActivity > 0;
  }

  /** Turns waiting for operator tool-round budget approval. */
  get pendingBudgetApprovals(): PendingBudgetApproval[] {
    const items: PendingBudgetApproval[] = [];
    for (const [turnId, turn] of this.turns) {
      if (turn.terminal) continue;
      if (
        turn.phase !== "budget_blocked" &&
        turn.phase !== "budget_approval" &&
        !turn.budgetRequestId
      ) {
        continue;
      }
      const requestId = turn.budgetRequestId?.trim();
      if (!requestId) continue;
      items.push({
        turnId,
        messageId: turn.messageId,
        requestId,
        workCardId: turn.workspaceCardId?.trim() || requestId,
        requestedRounds: turn.requestedRounds ?? null,
        message: "Medousa needs more tool rounds to finish this task.",
      });
    }
    return items;
  }

  clearBudgetAlert() {
    this.budgetAlert = null;
  }

  hasPendingBudgetApproval(requestId: string): boolean {
    const id = requestId.trim();
    if (!id) return false;
    if (this.budgetAlert?.requestId === id) return true;
    return this.pendingBudgetApprovals.some((item) => item.requestId === id);
  }

  noteBudgetResolved(requestId: string) {
    if (this.budgetAlert?.requestId === requestId) {
      this.budgetAlert = null;
    }
    const next = new Map(this.turns);
    for (const [turnId, turn] of next) {
      if (turn.budgetRequestId === requestId) {
        next.set(turnId, {
          ...turn,
          phase: "tool_loop",
          budgetRequestId: null,
          requestedRounds: null,
        });
      }
    }
    this.turns = next;
    if (this.backgroundActivity > 0) {
      this.backgroundActivity -= 1;
    }
  }

  /** Back-compat alias for live stream only (not background handoffs). */
  get isStreaming(): boolean {
    return this.liveStreamActive;
  }

  isPinned(sessionId: string): boolean {
    return this.pinnedIds.includes(sessionId);
  }

  currentSessionLabel(): string {
    const firstUser = this.messages.find((message) => message.role === "user");
    if (firstUser?.content.trim()) {
      const line = firstUser.content.trim().split("\n")[0];
      return line.length > 48 ? `${line.slice(0, 47)}…` : line;
    }

    const match = this.sessions.find((session) => session.session_id === this.sessionId);
    if (match) return formatSessionLabel(match);

    return "New conversation";
  }

  togglePin(sessionId: string) {
    if (this.pinnedIds.includes(sessionId)) {
      this.pinnedIds = this.pinnedIds.filter((id) => id !== sessionId);
    } else {
      this.pinnedIds = [...this.pinnedIds, sessionId];
    }
    localStorage.setItem(PINS_KEY, JSON.stringify(this.pinnedIds));
  }

  async renameSession(sessionId: string, displayName: string): Promise<void> {
    const trimmed = displayName.trim();
    if (!trimmed) {
      throw new Error("Session name must not be empty");
    }
    const response = await setSessionDisplayName(sessionId, trimmed);
    this.sessions = this.sessions.map((session) =>
      session.session_id === sessionId
        ? { ...session, display_name: response.display_name }
        : session,
    );
  }

  async deleteSession(sessionId: string, options?: { purgeMemory?: boolean }) {
    const trimmed = sessionId.trim();
    if (!trimmed) {
      throw new Error("session_id is required");
    }
    await daemonDeleteSession(trimmed, options);
    this.sessions = this.sessions.filter((session) => session.session_id !== trimmed);
    this.pinnedIds = this.pinnedIds.filter((id) => id !== trimmed);
    localStorage.setItem(PINS_KEY, JSON.stringify(this.pinnedIds));
    if (this.sessionId === trimmed) {
      await this.newSession();
    } else {
      await this.refreshSessions({ force: true });
    }
  }

  async refreshSessions(options?: { force?: boolean; q?: string }) {
    const force = options?.force ?? false;
    const query = (options?.q ?? this.sessionListQuery).trim();
    if (options?.q !== undefined) {
      this.sessionListQuery = query;
    }
    const hadCache = this.sessions.length > 0;
    const fresh =
      !force &&
      !query &&
      hadCache &&
      Date.now() - this.sessionsFetchedAt < SESSIONS_STALE_MS;

    if (fresh) {
      return;
    }

    if (this.sessionsRefreshInFlight) {
      return this.sessionsRefreshInFlight;
    }

    this.sessionsRefreshInFlight = this.fetchSessions(hadCache, query);
    try {
      await this.sessionsRefreshInFlight;
    } finally {
      this.sessionsRefreshInFlight = null;
    }
  }

  /** Debounced refresh after turn lifecycle events (coalesces rapid stream terminals). */
  scheduleSessionsRefresh() {
    if (this.sessionsRefreshTimer) {
      clearTimeout(this.sessionsRefreshTimer);
    }
    this.sessionsRefreshTimer = setTimeout(() => {
      this.sessionsRefreshTimer = null;
      void this.refreshSessions({ force: true });
    }, SESSIONS_REFRESH_DEBOUNCE_MS);
  }

  private async fetchSessions(hadCache: boolean, query = "") {
    this.sessionsRefreshing = hadCache;
    if (!hadCache) {
      this.sessionsError = null;
    }
    try {
      const response = await listSessions({
        limit: 50,
        includeVerification: false,
        q: query || undefined,
      });
      this.sessions = response.sessions;
      this.sessionsFetchedAt = Date.now();
      this.sessionsError = null;
    } catch (err) {
      if (!hadCache) {
        this.sessionsError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      this.sessionsRefreshing = false;
    }
  }

  async newSession() {
    this.transcriptEpoch += 1;
    this.historyLoading = false;
    this.sessionPristine = true;
    const id = `medousa-home-${crypto.randomUUID()}`;
    localStorage.setItem(SESSION_KEY, id);
    this.sessionId = id;
    this.messages = [];
    this.streamError = null;
    this.historyNotice = null;
    this.backgroundActivity = 0;
    this.activeTurnId = null;
    this.turns = new Map();
    this.workers = new Map();
    void this.clearStreamOwnership();
    await this.refreshSessions({ force: true });
  }

  /** Pull transcript from the daemon when the UI remounted empty (startup / reconnect). */
  async ensureSessionHydrated(options?: { notice?: boolean }) {
    if (this.historyLoading) return;
    if (this.sessionPristine) return;
    if (this.messages.length === 0) {
      await this.reloadCurrentSession(options);
      return;
    }
    await this.reconcileOnResume({ notice: options?.notice });
    this.sanitizeTranscript();
  }

  /** Drop duplicate message ids — keyed chat UI throws if ids repeat. */
  sanitizeTranscript() {
    const deduped = dedupeMessagesById(this.messages);
    if (deduped.length !== this.messages.length) {
      this.messages = deduped;
    }
  }

  /** Foreground/resume: merge daemon history with local stream state and reattach SSE. */
  async reconcileOnResume(options?: { notice?: boolean }, cards: WorkCard[] = []) {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    const epoch = this.transcriptEpoch;
    try {
      const attached = await this.tryReattachActiveTurn(cards);
      if (epoch !== this.transcriptEpoch) return;

      const liveStream =
        this.messages.some((message) => message.streaming) ||
        [...this.turns.values()].some(
          (turn) => !turn.terminal && turn.mode === "interactive",
        );

      // Merging daemon history mid-stream duplicates local user/assistant bubbles.
      if (liveStream) {
        this.sanitizeTranscript();
        if (attached && options?.notice !== false && this.historyNotice == null) {
          this.historyNotice = "Reconnected to live turn";
        }
        return;
      }

      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;

      const daemonMessages = mapTurns(history.turns, { sessionId });
      this.messages = mergeTranscript(this.messages, daemonMessages);
      this.sanitizeTranscript();

      if (attached && options?.notice !== false && this.historyNotice == null) {
        this.historyNotice = "Reconnected to live turn";
      }
    } catch (err) {
      this.noteResumeFailure(err);
    }
  }

  /** Fetch current session history from the engine (survives WebView refresh). */
  async reloadCurrentSession(options?: { notice?: boolean }) {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    const epoch = this.transcriptEpoch;
    this.historyLoading = true;
    this.streamError = null;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;
      this.messages = mapTurns(history.turns, { sessionId });
      if (options?.notice !== false && history.turns.length > 0) {
        const count = history.turns.length;
        this.historyNotice = `Restored ${count} turn${count === 1 ? "" : "s"}`;
      }
    } catch (err) {
      if (epoch === this.transcriptEpoch) {
        this.streamError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (epoch === this.transcriptEpoch) {
        this.historyLoading = false;
      }
    }
  }

  async switchSession(sessionId: string) {
    if (sessionId === this.sessionId) {
      await this.reloadCurrentSession({ notice: false });
      return;
    }
    this.sessionPristine = false;
    this.transcriptEpoch += 1;
    this.sessionId = sessionId;
    localStorage.setItem(SESSION_KEY, sessionId);
    this.streamError = null;
    this.historyNotice = null;
    this.messages = [];
    this.backgroundActivity = 0;
    this.activeTurnId = null;
    this.turns = new Map();
    this.workers = new Map();
    void this.clearStreamOwnership();
    this.historyLoading = true;
    const epoch = this.transcriptEpoch;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;
      this.messages = mapTurns(history.turns, { sessionId });
      const { workshops } = await import("$lib/stores/workshops.svelte");
      void workshops.saveActiveSession(sessionId);
    } catch (err) {
      if (epoch === this.transcriptEpoch) {
        this.streamError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (epoch === this.transcriptEpoch) {
        this.historyLoading = false;
      }
    }
    void this.tryReattachActiveTurn();
  }

  clearHistoryNotice() {
    this.historyNotice = null;
  }

  noteTurnStarted(turnId: string) {
    this.activeTurnId = turnId;
  }

  registerTurn(ticket: TurnTicketResponse, messageId: string | null) {
    this.activeTurnId =
      ticket.mode === "interactive" ? ticket.turn_id : this.activeTurnId;
    const next = new Map(this.turns);
    next.set(ticket.turn_id, {
      turnId: ticket.turn_id,
      mode: ticket.mode,
      phase: ticket.phase,
      messageId,
      streamAttached: true,
      terminal: false,
      workspaceCardId: ticket.workspace_card_id ?? null,
    });
    this.turns = next;
  }

  /** Start a user + assistant bubble pair for a new turn ticket. */
  beginTurn(
    userContent: string,
    ticket: TurnTicketResponse,
    mediaRefs: MediaRef[] = [],
  ) {
    this.sessionPristine = false;
    this.transcriptEpoch += 1;
    this.historyNotice = null;
    const assistantId = crypto.randomUUID();
    const isAsk = ticket.mode === "background";
    const askJobId = ticket.workspace_card_id ?? ticket.turn_id;
    const lane = isAsk ? ("ask" as const) : ("chat" as const);
    this.messages = [
      ...this.messages,
      {
        id: crypto.randomUUID(),
        role: "user",
        content: userContent,
        turnId: ticket.turn_id,
        lane,
        askJobId: isAsk ? askJobId : null,
        mediaAttachments:
          mediaRefs.length > 0
            ? chatMediaAttachmentsFromRefs(mediaRefs)
            : undefined,
      },
      {
        id: assistantId,
        role: "assistant",
        content: "",
        streaming: true,
        turnId: ticket.turn_id,
        lane,
        askJobId: isAsk ? askJobId : null,
        statusLine:
          ticket.mode === "background" ? "Background turn started" : null,
      },
    ];
    this.registerTurn(ticket, assistantId);
    if (ticket.mode === "interactive") {
      this.assistantId = assistantId;
      this.activeTurnId = ticket.turn_id;
    } else {
      this.backgroundActivity += 1;
    }
    this.streamError = null;
  }

  registerTurnFromRecord(record: TurnTicketRecord, messageId: string | null) {
    this.registerTurn(
      {
        turn_id: record.turn_id,
        session_id: record.session_id,
        mode: record.mode,
        phase: record.phase,
        accepted_at_utc: record.started_at,
        stream_url: record.stream_url,
        stream_ready: true,
        workspace_card_id: record.workspace_card_id ?? null,
      },
      messageId,
    );
  }

  clearActiveTurn() {
    this.activeTurnId = null;
  }

  /** Bind local SSE ownership after the shell starts the daemon stream URL. */
  async startTurnStream(turnId: string, sessionId: string, streamUrl: string) {
    await startInteractiveStream(streamUrl);
    this.markStreamOwner(turnId, sessionId, streamUrl);
  }

  /**
   * Reattach SSE listeners for owned, non-terminal session turns after refresh/reconnect.
   */
  async tryReattachActiveTurn(cards: WorkCard[] = []): Promise<boolean> {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return false;

    await this.pruneStreamOwnership();

    try {
      const targets: TurnTicketRecord[] = [];
      const response = await listSessionTurns(sessionId, true);
      if (response.turns.length === 0) {
        const legacy = await getActiveSessionTurn(sessionId);
        if (!legacy.active || !legacy.turn) {
          this.activeTurnId = null;
          return false;
        }
        response.turns.push({
          turn_id: legacy.turn.turn_id,
          session_id: legacy.turn.session_id,
          mode: "interactive",
          phase: "streaming",
          stream_url: legacy.turn.stream_url,
          prompt_preview: "",
          workspace_card_id: null,
          composer_handoff: legacy.turn.composer_handoff,
          started_at: legacy.turn.started_at,
          updated_at: legacy.turn.started_at,
        });
      }
      targets.push(...response.turns);

      for (const card of cards) {
        if (!isAskJobId(card.id)) continue;
        if (card.column === "done" || card.column === "blocked") continue;
        try {
          const askResponse = await listSessionTurns(askSessionId(card.id), true);
          targets.push(...askResponse.turns);
        } catch {
          // Best-effort — card may still be queued.
        }
      }

      let attached = false;
      const seen = new Set<string>();
      for (const record of targets) {
        if (seen.has(record.turn_id)) continue;
        seen.add(record.turn_id);
        if (await this.attachTurnStream(record)) {
          attached = true;
        }
      }

      await this.pruneStreamOwnership();
      return attached;
    } catch (err) {
      this.noteResumeFailure(err);
      return false;
    }
  }

  /** @deprecated use tryReattachActiveTurn(cards) */
  async tryReattachAskTurns(cards: WorkCard[]): Promise<boolean> {
    return this.tryReattachActiveTurn(cards);
  }

  private reattachContextFor(record: TurnTicketRecord) {
    const assistant = this.messages.find(
      (message) => message.turnId === record.turn_id && message.role === "assistant",
    );
    return {
      principalSessionId: this.sessionId,
      isRelevantSession: (sessionId: string | null | undefined) =>
        this.isRelevantSession(sessionId),
      isDetachedWorkerTurn: (ticket: TurnTicketRecord) =>
        this.isDetachedWorkerTurnRecord(ticket),
      localTurn: this.turns.get(record.turn_id),
      hasAssistantMessage: assistant != null,
      assistantStreaming: assistant?.streaming ?? false,
    };
  }

  private markStreamOwner(turnId: string, sessionId: string, streamUrl: string) {
    this.streamOwners.set(turnId, { turnId, sessionId, streamUrl });
  }

  private async detachStreamOwner(turnId: string) {
    if (!this.streamOwners.delete(turnId)) return;
    try {
      await stopInteractiveStreamTurn(turnId);
    } catch {
      // Best-effort detach.
    }
  }

  private async clearStreamOwnership() {
    const turnIds = [...this.streamOwners.keys()];
    this.streamOwners.clear();
    await Promise.all(
      turnIds.map((turnId) =>
        stopInteractiveStreamTurn(turnId).catch(() => undefined),
      ),
    );
  }

  /** Stop every turn-scoped SSE listener Home owns (keeps Rust slots in sync). */
  async stopOwnedInteractiveStreams(): Promise<void> {
    await this.clearStreamOwnership();
  }

  private async pruneStreamOwnership() {
    for (const [turnId] of this.streamOwners) {
      const turn = this.turns.get(turnId);
      if (!turn || turn.terminal) {
        await this.detachStreamOwner(turnId);
        continue;
      }
      if (turn.phase === "worker_handoff" && turn.mode === "interactive") {
        await this.detachStreamOwner(turnId);
      }
    }
  }

  private async attachTurnStream(record: TurnTicketRecord): Promise<boolean> {
    if (!shouldReattachTurnRecord(record, this.reattachContextFor(record))) {
      return false;
    }

    const existing = this.streamOwners.get(record.turn_id);
    if (existing?.streamUrl === record.stream_url) {
      return false;
    }
    if (existing) {
      await this.detachStreamOwner(record.turn_id);
    }

    let messageId = this.messages.find(
      (message) => message.turnId === record.turn_id && message.role === "assistant",
    )?.id;

    if (!messageId && !record.composer_handoff) {
      messageId = crypto.randomUUID();
      const lane = record.mode === "background" ? ("ask" as const) : ("chat" as const);
      const askJobId =
        record.mode === "background"
          ? (record.workspace_card_id ?? record.turn_id)
          : null;
      this.messages = [
        ...this.messages,
        {
          id: messageId,
          role: "assistant",
          content: "",
          streaming: true,
          turnId: record.turn_id,
          lane,
          askJobId,
        },
      ];
      if (record.mode === "interactive") {
        this.assistantId = messageId;
      }
    } else if (messageId && record.mode === "interactive") {
      this.assistantId = messageId;
    }

    this.registerTurnFromRecord(record, messageId ?? null);
    if (record.composer_handoff && record.mode === "interactive") {
      this.backgroundActivity = Math.max(this.backgroundActivity, 1);
    } else if (record.mode === "background") {
      this.backgroundActivity = Math.max(this.backgroundActivity, 1);
    }

    await startInteractiveStream(record.stream_url);
    this.markStreamOwner(record.turn_id, record.session_id, record.stream_url);
    return true;
  }

  /** Cancel the daemon-side turn and detach the local SSE listener. */
  async cancelActiveTurn(): Promise<void> {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    const turnId = this.activeTurnId;

    try {
      await cancelActiveSessionTurn(sessionId);
    } catch {
      // Best-effort — still settle local state below.
    }

    if (turnId) {
      if (this.assistantId) {
        this.finishMessage(this.assistantId);
      }
      this.settleTurn(turnId);
      return;
    }

    const ownedTurnIds = [...this.streamOwners.entries()]
      .filter(([, owner]) => owner.sessionId === sessionId)
      .map(([id]) => id);
    this.evictStreamOwners(ownedTurnIds);
    for (const ownedTurnId of ownedTurnIds) {
      await stopInteractiveStreamTurn(ownedTurnId).catch(() => undefined);
    }
    this.activeTurnId = null;
    this.assistantId = null;
  }

  /** Workspace/worker or budget card settled — drop one background pulse unit. */
  noteBackgroundSettled(count = 1) {
    this.backgroundActivity = Math.max(0, this.backgroundActivity - count);
  }

  /** Ask job reached a terminal workspace column — close the ask lane turn. */
  noteAskTurnSettled(jobId: string) {
    const trimmed = jobId.trim();
    if (!trimmed) return;

    let settledTurn = false;
    for (const [turnId, turn] of this.turns) {
      if (turn.mode !== "background") continue;
      if (turn.workspaceCardId !== trimmed && turnId !== trimmed) continue;
      this.settleTurn(turnId);
      settledTurn = true;
    }

    this.messages = this.messages.map((message) =>
      message.askJobId === trimmed && message.streaming
        ? { ...message, streaming: false, phase: null, statusLine: null }
        : message,
    );
    if (!settledTurn) {
      this.noteBackgroundSettled();
    }
  }

  /** Move a finished ask thread into the principal chat transcript. */
  promoteAskToChat(jobId: string) {
    const trimmed = jobId.trim();
    if (!trimmed) return;
    this.messages = dedupeMessagesById(
      this.messages.map((message) =>
        message.askJobId === trimmed
          ? { ...message, lane: "chat", askJobId: null }
          : message,
      ),
    );
  }

  /** Load isolated ask session transcripts into the Asks rail. */
  async hydrateAskThreads(cards: WorkCard[]) {
    const epoch = this.transcriptEpoch;
    const targets = cards.filter((card) => {
      if (!isAskJobId(card.id)) return false;
      if (this.askHydrationInFlight.has(card.id)) return false;
      return !this.messages.some((message) => message.askJobId === card.id);
    });
    if (targets.length === 0) return;

    for (const card of targets) {
      this.askHydrationInFlight.add(card.id);
    }

    try {
      const batches = await Promise.all(
        targets.map(async (card) => {
          try {
            const sessionId = askSessionId(card.id);
            const history = await getSessionHistory(sessionId);
            if (epoch !== this.transcriptEpoch || history.turns.length === 0) {
              return [] as ChatMessage[];
            }
            return mapTurns(history.turns, {
              lane: "ask",
              askJobId: card.id,
              sessionId,
            });
          } catch {
            return [] as ChatMessage[];
          }
        }),
      );

      if (epoch !== this.transcriptEpoch) return;

      const hydrated = batches.flat();
      if (hydrated.length === 0) return;

      const jobsAlreadyHydrated = new Set(
        this.messages
          .map((message) => message.askJobId)
          .filter((jobId): jobId is string => Boolean(jobId?.trim())),
      );
      const fresh = hydrated.filter(
        (message) => !message.askJobId || !jobsAlreadyHydrated.has(message.askJobId),
      );
      if (fresh.length === 0) return;

      this.messages = dedupeMessagesById([...this.messages, ...fresh]);
    } finally {
      for (const card of targets) {
        this.askHydrationInFlight.delete(card.id);
      }
    }
  }

  private isRelevantSession(sessionId: string | null | undefined): boolean {
    const trimmed = sessionId?.trim();
    if (!trimmed) return false;
    if (trimmed === this.sessionId) return true;

    for (const link of this.workers.values()) {
      if (link.sessionId === trimmed) return true;
    }

    const jobId = askJobIdFromSession(trimmed);
    if (!jobId) return false;

    if (this.messages.some((message) => message.askJobId === jobId)) {
      return true;
    }

    for (const turn of this.turns.values()) {
      if (turn.workspaceCardId === jobId) return true;
    }

    return false;
  }

  private resolveTurnSessionId(
    turnId: string | null | undefined,
    workspaceCardId?: string | null,
  ): string {
    const cardId = workspaceCardId?.trim();
    if (cardId && isAskJobId(cardId)) {
      return askSessionId(cardId);
    }
    if (turnId) {
      const turn = this.turns.get(turnId);
      if (turn?.workspaceCardId && isAskJobId(turn.workspaceCardId)) {
        return askSessionId(turn.workspaceCardId);
      }
      if (turn?.mode === "background" && isAskJobId(turnId)) {
        return askSessionId(turnId);
      }
    }
    return this.sessionId;
  }

  private linkWorker(params: {
    workId: string;
    parentTurnId: string | null;
    messageId: string | null;
    sessionId: string;
  }) {
    const existing = this.workers.get(params.workId);
    const link: WorkerLink = {
      workId: params.workId,
      parentTurnId: params.parentTurnId ?? existing?.parentTurnId ?? null,
      messageId: params.messageId ?? existing?.messageId ?? null,
      synthesisMessageId: existing?.synthesisMessageId ?? null,
      sessionId: params.sessionId,
      synthesisDelivered: existing?.synthesisDelivered ?? false,
    };
    const nextWorkers = new Map(this.workers);
    nextWorkers.set(params.workId, link);
    this.workers = nextWorkers;

    if (params.parentTurnId) {
      const turn = this.turns.get(params.parentTurnId);
      if (turn) {
        const nextTurns = new Map(this.turns);
        nextTurns.set(params.parentTurnId, {
          ...turn,
          workspaceCardId: params.workId,
        });
        this.turns = nextTurns;
      }
    }
  }

  linkWorkerFromStream(event: InteractiveTurnStreamEvent, messageId: string) {
    const workId = event.work_id?.trim();
    if (!workId) return;
    this.linkWorker({
      workId,
      parentTurnId: event.turn_id,
      messageId,
      sessionId: this.resolveTurnSessionId(event.turn_id),
    });
    const followUpId = this.ensureWorkerFollowUpBubble(workId, event.turn_id, {
      statusLine: "Working in background…",
      streaming: true,
    });
    const link = this.workers.get(workId);
    if (link && link.synthesisMessageId !== followUpId) {
      const nextWorkers = new Map(this.workers);
      nextWorkers.set(workId, { ...link, synthesisMessageId: followUpId });
      this.workers = nextWorkers;
    }
  }

  onWorkerCardDetail(
    detail: WorkCardDetail,
    column: string,
    previousColumn: string | undefined,
  ) {
    if (detail.kind !== "turn_worker") return;
    const sessionId = detail.session_id?.trim();
    if (!sessionId || !this.isRelevantSession(sessionId)) return;

    const workId = detail.work_id?.trim() || detail.card.id;
    const parentTurnId = detail.correlation_id?.trim() || null;
    const messageId = parentTurnId ? this.messageIdForTurn(parentTurnId) : null;
    this.linkWorker({
      workId,
      parentTurnId,
      messageId,
      sessionId,
    });

    if (column === "wrapping_up" && previousColumn !== "wrapping_up") {
      this.noteWorkerSynthesizing(workId);
    }
    if (
      (column === "done" || (column === "blocked" && detail.terminal)) &&
      previousColumn !== column
    ) {
      void this.deliverWorkerSynthesis(workId, detail);
    }
  }

  /** After hydrate/reconnect — deliver syntheses that landed while SSE was detached. */
  async recoverPendingWorkerSyntheses(
    cards: WorkCard[],
    details: Map<string, WorkCardDetail>,
  ) {
    for (const card of cards) {
      const detail = details.get(card.id);
      if (!detail || detail.kind !== "turn_worker") continue;
      if (!this.isRelevantSession(detail.session_id?.trim())) continue;
      this.onWorkerCardDetail(detail, card.column, undefined);
      const workId = detail.work_id?.trim() || card.id;
      const link = this.workers.get(workId);
      if (link && !link.synthesisDelivered && card.column === "done") {
        await this.deliverWorkerSynthesis(workId, detail);
      }
    }
  }

  /** Keep worker-lane bubbles in sync with workspace card columns (no parent SSE). */
  syncWorkerLaneFromCards(
    cards: WorkCard[],
    details: Map<string, WorkCardDetail>,
  ) {
    for (const card of cards) {
      const detail = details.get(card.id);
      if (!detail || detail.kind !== "turn_worker") continue;
      if (!this.isRelevantSession(detail.session_id?.trim())) continue;
      const workId = detail.work_id?.trim() || card.id;
      const statusLine = workerStatusLineForColumn(card.column);
      const streaming =
        card.column === "backlog" || card.column === "in_flight";
      this.updateWorkerLaneBubble(workId, { statusLine, streaming });
    }
  }

  private updateWorkerLaneBubble(
    workId: string,
    options: { statusLine: string; streaming: boolean },
  ) {
    const link = this.workers.get(workId);
    const targetId = link?.synthesisMessageId;
    if (!targetId) return;
    const idx = this.messages.findIndex((message) => message.id === targetId);
    if (idx < 0) return;
    const current = this.messages[idx];
    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        lane: "worker",
        workId,
        statusLine: options.statusLine,
        streaming: options.streaming && !current.content.trim(),
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private noteWorkerSynthesizing(workId: string) {
    const link = this.workers.get(workId);
    if (!link?.messageId) return;

    const idx = this.messages.findIndex((m) => m.id === link.messageId);
    if (idx < 0) return;

    const current = this.messages[idx];
    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        streaming: true,
        statusLine: "Pulling that together…",
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private finalizeWorkerHandoffBubble(messageId: string | null) {
    if (!messageId) return;
    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx < 0) return;
    const current = this.messages[idx];
    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        streaming: false,
        phase: null,
        statusLine: null,
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private workerLinkForTurn(turnId: string): WorkerLink | undefined {
    for (const link of this.workers.values()) {
      if (link.parentTurnId === turnId) return link;
    }
    return undefined;
  }

  private ensureWorkerFollowUpBubble(
    workId: string,
    turnId: string | null,
    options?: { statusLine?: string | null; streaming?: boolean },
  ): string {
    const link = this.workers.get(workId);

    if (link?.synthesisMessageId) {
      const existingIdx = this.messages.findIndex(
        (message) => message.id === link.synthesisMessageId,
      );
      if (existingIdx >= 0) {
        const current = this.messages[existingIdx];
        this.messages = [
          ...this.messages.slice(0, existingIdx),
          {
            ...current,
            streaming: options?.streaming ?? true,
            statusLine: options?.statusLine ?? current.statusLine,
          },
          ...this.messages.slice(existingIdx + 1),
        ];
        return link.synthesisMessageId;
      }
    }

    const handoffMessage = link?.messageId
      ? this.messages.find((message) => message.id === link.messageId)
      : null;
    const turn = turnId ? this.turns.get(turnId) : null;

    const id = crypto.randomUUID();
    this.messages = [
      ...this.messages,
      {
        id,
        role: "assistant",
        content: "",
        streaming: options?.streaming ?? true,
        turnId,
        lane: "worker",
        workId,
        statusLine: options?.statusLine ?? null,
      },
    ];

    if (link) {
      const nextWorkers = new Map(this.workers);
      nextWorkers.set(workId, { ...link, synthesisMessageId: id });
      this.workers = nextWorkers;
    }

    if (turnId) {
      const activeTurn = this.turns.get(turnId);
      if (activeTurn) {
        const nextTurns = new Map(this.turns);
        nextTurns.set(turnId, { ...activeTurn, messageId: id });
        this.turns = nextTurns;
      }
    }

    return id;
  }

  private hasFollowUpSynthesis(handoffMessageId: string | null, content: string): boolean {
    if (!handoffMessageId) return false;
    const handoffIdx = this.messages.findIndex((m) => m.id === handoffMessageId);
    if (handoffIdx < 0) return false;
    const target = content.trim();
    return this.messages.slice(handoffIdx + 1).some(
      (message) =>
        message.role === "assistant" && message.content.trim() === target,
    );
  }

  private removeMessageById(messageId: string | null | undefined) {
    if (!messageId) return;
    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx < 0) return;
    this.messages = [
      ...this.messages.slice(0, idx),
      ...this.messages.slice(idx + 1),
    ];
  }

  private async resolveWorkerSynthesisContent(
    link: WorkerLink,
    detail?: WorkCardDetail,
  ): Promise<string | null> {
    const handoffMessage = link.messageId
      ? this.messages.find((message) => message.id === link.messageId)
      : null;
    const handoffContent =
      handoffMessage?.stageWhisper?.trim() || handoffMessage?.content || null;
    const fromHistory = await this.fetchLatestAssistantTurn(link.sessionId, handoffContent);
    if (fromHistory) return fromHistory;

    const excerpt = detail?.result_excerpt?.trim();
    if (excerpt) return excerpt;

    return detail?.error?.trim() || null;
  }

  private async deliverWorkerSynthesis(workId: string, detail?: WorkCardDetail) {
    const link = this.workers.get(workId);
    if (!link || link.synthesisDelivered) return;

    const content = await this.resolveWorkerSynthesisContent(link, detail);
    if (!content) return;

    if (this.hasFollowUpSynthesis(link.messageId, content)) {
      this.finalizeWorkerHandoffBubble(link.messageId);
      this.markWorkerSynthesisDelivered(workId);
      this.noteBackgroundSettled();
      return;
    }

    const targetId =
      link.synthesisMessageId ??
      this.ensureWorkerFollowUpBubble(workId, link.parentTurnId, {
        streaming: false,
      });
    if (targetId) {
      const idx = this.messages.findIndex((m) => m.id === targetId);
      if (idx >= 0) {
        this.messages = [
          ...this.messages.slice(0, idx),
          {
            ...this.messages[idx],
            content,
            streaming: false,
            phase: null,
            statusLine: null,
            lane: "worker",
            workId,
            tools: detail?.tool_names?.length
              ? [...detail.tool_names]
              : this.messages[idx].tools,
          },
          ...this.messages.slice(idx + 1),
        ];
        this.finalizeWorkerHandoffBubble(link.messageId);
        this.markWorkerSynthesisDelivered(workId);
        if (link.parentTurnId) {
          const turn = this.turns.get(link.parentTurnId);
          if (turn?.mode === "background") {
            this.settleTurn(link.parentTurnId);
          } else {
            this.noteBackgroundSettled();
          }
        } else {
          this.noteBackgroundSettled();
        }
        return;
      }
    }

    this.appendWorkerSynthesisMessage(workId, link.parentTurnId, content, detail?.tool_names);
    this.markWorkerSynthesisDelivered(workId);
    this.noteBackgroundSettled();
  }

  private markWorkerSynthesisDelivered(workId: string) {
    const link = this.workers.get(workId);
    if (!link || link.synthesisDelivered) return;
    const nextWorkers = new Map(this.workers);
    nextWorkers.set(workId, { ...link, synthesisDelivered: true });
    this.workers = nextWorkers;
  }

  private shouldSettleTurnFromStream(turnId: string): boolean {
    const turn = this.turns.get(turnId);
    if (turn?.mode === "background") return false;
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink && !workerLink.synthesisDelivered) return false;
    return true;
  }

  private settleTurn(turnId: string) {
    const turn = this.turns.get(turnId);
    if (!turn) return;
    if (turn.mode === "background" || this.backgroundActivity > 0) {
      this.backgroundActivity = Math.max(0, this.backgroundActivity - 1);
    }
    if (this.activeTurnId === turnId) {
      this.activeTurnId = null;
    }
    if (this.assistantId && turn.messageId === this.assistantId) {
      this.assistantId = null;
    }
    const next = new Map(this.turns);
    next.delete(turnId);
    this.turns = next;
    void this.detachStreamOwner(turnId);
  }

  private appendWorkerSynthesisMessage(
    workId: string,
    parentTurnId: string | null,
    content: string,
    toolNames?: string[] | null,
  ) {
    const link = this.workers.get(workId);
    const targetId = link?.messageId;
    if (targetId) {
      const idx = this.messages.findIndex((m) => m.id === targetId);
      if (idx >= 0) {
        this.messages = [
          ...this.messages.slice(0, idx),
          {
            ...this.messages[idx],
            content,
            streaming: false,
            phase: null,
            statusLine: null,
            tools: toolNames?.length ? [...toolNames] : this.messages[idx].tools,
          },
          ...this.messages.slice(idx + 1),
        ];
        if (link) {
          const nextWorkers = new Map(this.workers);
          nextWorkers.set(workId, { ...link, synthesisMessageId: targetId });
          this.workers = nextWorkers;
        }
        return;
      }
    }

    const id = crypto.randomUUID();
    this.messages = [
      ...this.messages,
      {
        id,
        role: "assistant",
        content,
        turnId: parentTurnId,
        tools: toolNames?.length ? [...toolNames] : undefined,
      },
    ];
    if (link) {
      const nextWorkers = new Map(this.workers);
      nextWorkers.set(workId, { ...link, synthesisMessageId: id });
      this.workers = nextWorkers;
    }
  }

  private async fetchLatestAssistantTurn(
    sessionId: string,
    skipContentMatching?: string | null,
  ): Promise<string | null> {
    try {
      const history = await getSessionHistory(sessionId);
      const assistants = [...history.turns].reverse().filter((turn) => turn.role === "assistant");
      const skip = skipContentMatching?.trim();
      if (skip) {
        const handoffTurn = assistants.find((turn) => turn.content.trim() === skip);
        if (handoffTurn) {
          const handoffIdx = history.turns.indexOf(handoffTurn);
          const after = history.turns
            .slice(handoffIdx + 1)
            .reverse()
            .find((turn) => turn.role === "assistant");
          return after?.content?.trim() || null;
        }
      }
      return assistants[0]?.content?.trim() || null;
    } catch {
      return null;
    }
  }

  applyStreamEvent(event: InteractiveTurnStreamEvent) {
    if (!this.isRelevantStreamEvent(event)) return;

    if (event.event_type === "error") {
      this.handleTurnError(event);
      return;
    }

    this.syncTurnFromEvent(event);

    const workerLink = this.workerLinkForTurn(event.turn_id);
    if (event.terminal && workerLink?.synthesisDelivered) {
      this.settleTurn(event.turn_id);
      return;
    }

    if (event.event_type === "tool_started" || event.event_type === "tool_finished") {
      const messageId = this.messageIdForToolStream(event.turn_id);
      if (messageId) {
        this.applyToolStreamEvent(messageId, event);
      }
      return;
    }

    if (event.event_type === "artifact_presented") {
      const messageId = this.messageIdForTurn(event.turn_id);
      if (messageId && event.ui_artifact) {
        this.applyArtifactPresented(messageId, event.ui_artifact);
      }
      return;
    }

    if (event.event_type === "artifact_updated") {
      const messageId = this.messageIdForTurn(event.turn_id);
      if (messageId && event.ui_artifact && event.previous_artifact_id) {
        this.applyArtifactUpdated(
          messageId,
          event.previous_artifact_id,
          event.root_artifact_id ?? null,
          event.ui_artifact,
        );
      }
      return;
    }

    if (event.event_type === "scratch_reset") {
      const messageId = this.messageIdForTurn(event.turn_id);
      if (messageId) {
        this.applyStreamEventToMessage(messageId, event);
      }
      return;
    }

    const messageId = this.messageIdForTurn(event.turn_id);
    if (messageId) {
      this.applyStreamEventToMessage(messageId, event);
      return;
    }

    if (event.content_delta || event.final_text || event.event_type === "content_delta") {
      this.attachOrphanStream(event);
      return;
    }

    if (event.terminal) {
      this.noteTurnTerminal(event);
      this.finishAskLaneTurn(event.turn_id);
    }
  }

  private handleTurnError(event: InteractiveTurnStreamEvent) {
    this.streamError = operatorStreamErrorLine(
      event,
      settings.showEngineDetailsInChat,
    );

    const messageId = this.messageIdForTurn(event.turn_id);
    if (messageId) {
      this.messages = this.messages.filter((message) => message.id !== messageId);
      if (this.assistantId === messageId) {
        this.assistantId = null;
      }
    }

    this.finishAskLaneTurn(event.turn_id);
    this.noteTurnTerminal(event);
    if (this.shouldSettleTurnFromStream(event.turn_id)) {
      this.settleTurn(event.turn_id);
    }
  }

  private messageIdForTurn(turnId: string): string | null {
    const turn = this.turns.get(turnId);
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink && !workerLink.synthesisDelivered) {
      if (workerLink.synthesisMessageId) {
        return workerLink.synthesisMessageId;
      }
      if (workerLink.messageId) {
        return workerLink.messageId;
      }
    }
    if (turn?.messageId) return turn.messageId;
    if (workerLink?.synthesisMessageId) return workerLink.synthesisMessageId;
    return (
      this.messages.find(
        (message) => message.turnId === turnId && message.role === "assistant",
      )?.id ?? null
    );
  }

  /** Route worker tool receipts into the same turn envelope as synthesis. */
  private messageIdForToolStream(turnId: string): string | null {
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink && !workerLink.synthesisDelivered) {
      if (workerLink.messageId) {
        return workerLink.messageId;
      }
      return this.ensureWorkerFollowUpBubble(workerLink.workId, turnId, {
        statusLine: "Working in background…",
        streaming: true,
      });
    }
    return this.messageIdForTurn(turnId);
  }

  private applyToolStreamEvent(messageId: string, event: InteractiveTurnStreamEvent) {
    const idx = this.messages.findIndex((message) => message.id === messageId);
    if (idx < 0) return;

    const runId = event.tool_run_id?.trim();
    const toolName = event.tool_name?.trim();
    if (!runId || !toolName) return;

    const current = this.messages[idx];
    const runs = [...(current.toolRuns ?? [])];
    const existingIdx = runs.findIndex((run) => run.runId === runId);
    const round = event.tool_round ?? 1;

    if (event.event_type === "tool_started") {
      const next: ToolRunState = {
        runId,
        toolName,
        status: "running",
        round,
        inputSummary: event.tool_input_summary ?? null,
      };
      if (existingIdx >= 0) {
        runs[existingIdx] = { ...runs[existingIdx], ...next };
      } else {
        runs.push(next);
      }
    } else {
      const status: ToolRunState["status"] =
        event.tool_status === "failed" ? "failed" : "succeeded";
      const next: ToolRunState = {
        runId,
        toolName,
        status,
        round,
        inputSummary:
          event.tool_input_summary ?? runs[existingIdx]?.inputSummary ?? null,
        outputSummary: event.tool_output_summary ?? null,
        artifactRefs: event.tool_artifact_refs ?? undefined,
      };
      if (existingIdx >= 0) {
        runs[existingIdx] = { ...runs[existingIdx], ...next };
      } else {
        runs.push(next);
      }
    }

    runs.sort((a, b) => a.round - b.round || a.toolName.localeCompare(b.toolName));

    const tools = [...(current.tools ?? [])];
    if (!tools.includes(toolName)) {
      tools.push(toolName);
    }

    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        toolRuns: runs,
        tools: tools.length > 0 ? tools : current.tools,
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private applyArtifactPresented(
    messageId: string,
    artifact: NonNullable<InteractiveTurnStreamEvent["ui_artifact"]>,
  ) {
    const idx = this.messages.findIndex((message) => message.id === messageId);
    if (idx < 0) return;

    const current = this.messages[idx];
    const nextArtifact = mapStreamUiArtifact(artifact);

    const existing = current.uiArtifacts ?? [];
    if (existing.some((item) => item.artifactId === nextArtifact.artifactId)) {
      return;
    }

    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        uiArtifacts: [...existing, nextArtifact],
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private applyArtifactUpdated(
    messageId: string,
    previousArtifactId: string,
    rootArtifactId: string | null,
    artifact: NonNullable<InteractiveTurnStreamEvent["ui_artifact"]>,
  ) {
    const idx = this.messages.findIndex((message) => message.id === messageId);
    if (idx < 0) return;

    const current = this.messages[idx];
    const nextArtifact = mapStreamUiArtifact(artifact);
    const existing = current.uiArtifacts ?? [];
    const updated = replaceUiArtifactEntry(
      existing,
      previousArtifactId,
      rootArtifactId,
      nextArtifact,
    );

    this.messages = [
      ...this.messages.slice(0, idx),
      {
        ...current,
        uiArtifacts: updated,
      },
      ...this.messages.slice(idx + 1),
    ];
  }

  private applyStreamEventToMessage(
    messageId: string,
    event: InteractiveTurnStreamEvent,
  ) {
    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx < 0) {
      if (event.terminal) this.noteTurnTerminal(event);
      return;
    }

    const current = this.messages[idx];
    let content = current.content;

    if (event.event_type === "turn_progress") {
      const next: ChatMessage = {
        ...current,
        phase: "tool_loop",
        statusLine: this.resolveStatusLine(event, current.statusLine),
        tools: event.tool_names?.length
          ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
          : current.tools,
      };
      this.messages = [
        ...this.messages.slice(0, idx),
        next,
        ...this.messages.slice(idx + 1),
      ];
      return;
    }

    if (event.event_type === "turn_checkpoint") {
      const checkpointBody =
        event.final_text?.trim() ||
        event.message?.trim() ||
        current.content;
      const prior = current.content.trim();
      const merged =
        prior && checkpointBody && prior !== checkpointBody
          ? `${prior}\n\n${checkpointBody}`
          : resolveTurnContent(current.content, checkpointBody, true);
      const next: ChatMessage = {
        ...current,
        content: merged,
        phase: "handoff",
        statusLine:
          event.message?.trim() ||
          "Reply when you're ready — Medousa can continue this task.",
        tools: event.tool_names?.length
          ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
          : current.tools,
      };
      this.messages = [
        ...this.messages.slice(0, idx),
        next,
        ...this.messages.slice(idx + 1),
      ];
      if (event.terminal) {
        this.finishMessage(messageId);
        this.finishAskLaneTurn(event.turn_id);
        if (this.shouldSettleTurnFromStream(event.turn_id)) {
          this.settleTurn(event.turn_id);
          this.scheduleSessionsRefresh();
        }
      }
      return;
    }

    if (event.event_type === "scratch_reset") {
      const next: ChatMessage = {
        ...current,
        content: "",
        phase: "tool_loop",
        statusLine: current.statusLine,
      };
      this.messages = [
        ...this.messages.slice(0, idx),
        next,
        ...this.messages.slice(idx + 1),
      ];
      return;
    }

    if (event.content_delta) {
      if (!shouldSuppressStreamContentDelta(current)) {
        content += event.content_delta;
      }
    } else if (event.final_text) {
      const terminal = isTerminalContentCommit(event);
      const workerLink = this.workerLinkForTurn(event.turn_id);
      const isWorkerSynthesisOnEnvelope =
        workerLink != null &&
        messageId === workerLink.messageId &&
        terminal &&
        Boolean(event.final_text?.trim());
      const isWorkerSynthesisTarget =
        workerLink != null && messageId !== workerLink.messageId;
      content =
        (isWorkerSynthesisTarget || isWorkerSynthesisOnEnvelope) && terminal
          ? event.final_text!
          : resolveTurnContent(current.content, event.final_text, terminal);
    }

    let reasoning = current.reasoning ?? "";
    if (event.reasoning_delta) {
      reasoning += event.reasoning_delta;
    }

    const tools = [...(current.tools ?? [])];
    for (const name of event.tool_names ?? []) {
      if (!tools.includes(name)) tools.push(name);
    }

    const next: ChatMessage = {
      ...current,
      content,
      phase: event.phase || current.phase,
        statusLine: this.resolveStatusLine(event, current.statusLine),
      tools: tools.length > 0 ? tools : current.tools,
      reasoning: reasoning || current.reasoning,
    };
    this.messages = [
      ...this.messages.slice(0, idx),
      next,
      ...this.messages.slice(idx + 1),
    ];

    if (isWorkerHandoffStreamEvent(event)) {
      this.releaseComposerHandoff(messageId, "worker_ack", event);
      this.scheduleSessionsRefresh();
      return;
    }

    if (isBudgetApprovalStreamEvent(event)) {
      this.releaseComposerHandoff(messageId, "budget_approval", event);
      return;
    }

    if (event.terminal) {
      this.finishMessage(messageId);
      this.finishAskLaneTurn(event.turn_id);
      if (this.shouldSettleTurnFromStream(event.turn_id)) {
        this.settleTurn(event.turn_id);
        this.scheduleSessionsRefresh();
      }
    }
  }

  private finishAskLaneTurn(turnId: string) {
    this.messages = this.messages.map((message) =>
      message.turnId === turnId &&
      message.lane === "ask" &&
      message.streaming
        ? { ...message, streaming: false, phase: null, statusLine: null }
        : message,
    );
  }

  private noteTurnTerminal(event: InteractiveTurnStreamEvent) {
    if (!this.shouldSettleTurnFromStream(event.turn_id)) return;
    this.settleTurn(event.turn_id);
  }

  /** Resume stream after handoff (e.g. budget approved) with no active assistant bubble. */
  private attachOrphanStream(event: InteractiveTurnStreamEvent) {
    const workerLink = this.workerLinkForTurn(event.turn_id);
    if (workerLink?.messageId) {
      this.applyStreamEventToMessage(workerLink.messageId, event);
      return;
    }

    const content = event.final_text ?? event.content_delta ?? "";
    if (!content && !event.terminal && event.event_type !== "budget_approval") {
      return;
    }

    const id = crypto.randomUUID();
    const turn = this.turns.get(event.turn_id);
    this.messages = [
      ...this.messages,
      {
        id,
        role: "assistant",
        content,
        streaming: !event.terminal,
        turnId: event.turn_id,
        phase: event.phase || null,
        statusLine: this.resolveStatusLine(event, null),
        tools: event.tool_names?.length ? [...event.tool_names] : undefined,
      },
    ];
    if (turn) {
      const next = new Map(this.turns);
      next.set(event.turn_id, { ...turn, messageId: id });
      this.turns = next;
    }
    if (workerLink && !workerLink.synthesisMessageId) {
      const nextWorkers = new Map(this.workers);
      nextWorkers.set(workerLink.workId, {
        ...workerLink,
        synthesisMessageId: id,
      });
      this.workers = nextWorkers;
    }
    if (turn?.mode === "interactive" && !event.terminal) {
      this.assistantId = id;
    }

    if (isWorkerHandoffStreamEvent(event)) {
      this.releaseComposerHandoff(id, "worker_ack", event);
      this.scheduleSessionsRefresh();
      return;
    }

    if (isBudgetApprovalStreamEvent(event)) {
      this.releaseComposerHandoff(id, "budget_approval", event);
      return;
    }

    if (event.terminal) {
      this.finishMessage(id);
      this.finishAskLaneTurn(event.turn_id);
      if (this.shouldSettleTurnFromStream(event.turn_id)) {
        this.settleTurn(event.turn_id);
        this.scheduleSessionsRefresh();
      }
    }
  }

  private releaseComposerHandoff(
    messageId: string,
    phase: "worker_ack" | "budget_approval",
    event: InteractiveTurnStreamEvent,
  ) {
    const statusLine =
      event.message?.trim() ||
      (phase === "worker_ack"
        ? "Background worker started"
        : "Waiting for operator approval");

    const budgetRequestId =
      phase === "budget_approval" ? budgetRequestIdFromStreamEvent(event) : null;
    const requestedRounds =
      phase === "budget_approval" ? (event.requested_rounds ?? null) : null;

    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx >= 0) {
      const current = this.messages[idx];
      const ackText =
        current.content.trim() ||
        event.final_text?.trim() ||
        statusLine;
      this.messages = [
        ...this.messages.slice(0, idx),
        {
          ...current,
          streaming: false,
          phase: phase === "worker_ack" ? null : "budget_blocked",
          statusLine: phase === "worker_ack" ? null : statusLine,
          stageWhisper: phase === "worker_ack" ? ackText : current.stageWhisper,
          content: phase === "worker_ack" ? "" : ackText,
          budgetRequestId,
          requestedRounds,
        },
        ...this.messages.slice(idx + 1),
      ];
    }

    const turn = this.turns.get(event.turn_id);
    if (turn) {
      const next = new Map(this.turns);
      next.set(event.turn_id, {
        ...turn,
        phase: phase === "worker_ack" ? "worker_handoff" : "budget_blocked",
        messageId: phase === "worker_ack" ? null : messageId,
        workspaceCardId:
          phase === "budget_approval" && budgetRequestId
            ? budgetRequestId
            : turn.workspaceCardId,
        budgetRequestId,
        requestedRounds,
      });
      this.turns = next;
    }

    if (this.assistantId === messageId) {
      this.assistantId = null;
    }
    if (this.activeTurnId === event.turn_id) {
      this.activeTurnId = null;
    }
    this.backgroundActivity += 1;

    if (phase === "worker_ack") {
      void this.detachStreamOwner(event.turn_id);
      this.linkWorkerFromStream(event, messageId);
      return;
    }

    if (budgetRequestId) {
      const alert: PendingBudgetApproval = {
        turnId: event.turn_id,
        messageId,
        requestId: budgetRequestId,
        workCardId: budgetRequestId,
        requestedRounds,
        message: statusLine,
      };
      this.budgetAlert = alert;
    }
  }

  finishMessage(messageId: string) {
    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx >= 0) {
      const next = {
        ...this.messages[idx],
        streaming: false,
        phase: null,
        statusLine: null,
      };
      this.messages = [
        ...this.messages.slice(0, idx),
        next,
        ...this.messages.slice(idx + 1),
      ];
    }
    if (this.assistantId === messageId) {
      this.assistantId = null;
    }
  }

  /** Back-compat alias for the primary interactive bubble. */
  finishStream() {
    if (this.assistantId) {
      this.finishMessage(this.assistantId);
    }
  }

  setError(message: string) {
    this.streamError = friendlyUserError(message);
    if (this.assistantId) {
      this.finishMessage(this.assistantId);
    }
  }

  /** SSE / stream transport failure — evict stale owners so reattach can succeed. */
  noteStreamFailure(message: string, options?: { recoverable?: boolean }) {
    this.streamError = friendlyUserError(message);
    this.evictStreamOwners();
    if (options?.recoverable !== false) {
      return;
    }
    if (this.assistantId) {
      this.finishMessage(this.assistantId);
    }
    for (const [turnId, turn] of this.turns) {
      if (turn.terminal || turn.mode === "background") continue;
      if (turn.phase === "budget_blocked" || turn.phase === "worker_handoff") continue;
      this.settleTurn(turnId);
    }
  }

  noteResumeFailure(err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.warn("[chat] resume reconcile failed:", detail);
  }

  /** Drop local SSE ownership without stopping daemon streams (already dead). */
  evictStreamOwners(turnIds?: string[]) {
    if (turnIds) {
      for (const turnId of turnIds) {
        this.streamOwners.delete(turnId);
      }
      return;
    }
    this.streamOwners.clear();
  }

  prefillDraft(text: string) {
    this.draft = text;
  }

  prefillFromVaultNote(
    scope: VaultNoteContextScope,
    draft: string,
    options?: { pin?: boolean },
  ) {
    this.vaultNoteContext = scope;
    this.draft = draft;
    this.pinVaultNoteContext = options?.pin ?? false;
  }

  clearVaultNoteContext() {
    this.vaultNoteContext = null;
    this.pinVaultNoteContext = false;
  }

  syncScriptWorkbenchContext(scope: ScriptWorkbenchContextScope | null) {
    this.scriptWorkbenchContext = scope;
  }

  prefillFromScriptWorkbench(
    scope: ScriptWorkbenchContextScope,
    draft: string,
    options?: { pin?: boolean },
  ) {
    this.scriptWorkbenchContext = scope;
    this.draft = draft;
    this.pinScriptWorkbenchContext = options?.pin ?? false;
  }

  clearScriptWorkbenchContext() {
    this.scriptWorkbenchContext = null;
    this.pinScriptWorkbenchContext = false;
  }

  clearPendingMedia() {
    this.pendingMediaRefs = [];
  }

  removePendingMedia(mediaId: string) {
    this.pendingMediaRefs = this.pendingMediaRefs.filter(
      (ref) => ref.media_id !== mediaId,
    );
  }

  async attachFilesFromPicker() {
    if (this.pendingMediaUploading) return;
    const slots = MAX_MEDIA_REFS_PER_TURN - this.pendingMediaRefs.length;
    if (slots <= 0) {
      this.setError(
        friendlyUserError(`too many attachments (max ${MAX_MEDIA_REFS_PER_TURN})`),
      );
      return;
    }
    this.pendingMediaUploading = true;
    try {
      const { attachChatFiles } = await import("$lib/utils/chatMediaUpload");
      const refs = await attachChatFiles(this.sessionId, { maxNew: slots });
      if (refs.length > 0) {
        this.pendingMediaRefs = [...this.pendingMediaRefs, ...refs];
        this.streamError = null;
      }
    } catch (err) {
      this.setError(err instanceof Error ? err.message : String(err));
    } finally {
      this.pendingMediaUploading = false;
    }
  }

  private isDetachedWorkerTurnRecord(record: TurnTicketRecord): boolean {
    const cardId = record.workspace_card_id?.trim();
    if (cardId?.startsWith("work-")) {
      return true;
    }
    if (record.mode === "background" && cardId?.startsWith("medousa-daemon-ask-")) {
      return false;
    }
    return false;
  }

  private isComposerOpenDuringHandoff(turnId: string, phase: string): boolean {
    if (phase === "worker_handoff" || phase === "budget_blocked") {
      return true;
    }
    const workerLink = this.workerLinkForTurn(turnId);
    return workerLink != null && !workerLink.synthesisDelivered;
  }

  /** Ignore stray worker-session streams and orphan turn ids outside this chat. */
  private isRelevantStreamEvent(event: InteractiveTurnStreamEvent): boolean {
    const turnId = event.turn_id?.trim();
    if (!turnId) return false;

    if (isWorkerHandoffStreamEvent(event) || isBudgetApprovalStreamEvent(event)) {
      return true;
    }
    if (this.workerLinkForTurn(turnId)) return true;

    const workId = event.work_id?.trim();
    if (workId && this.workers.has(workId)) return true;

    return shouldAcceptStreamEvent(turnId, this.streamOwners, this.turns);
  }

  private syncTurnFromEvent(event: InteractiveTurnStreamEvent) {
    const existing = this.turns.get(event.turn_id);
    if (!existing) return;

    const workerLink = this.workerLinkForTurn(event.turn_id);
    const preserveHandoff =
      workerLink != null &&
      !workerLink.synthesisDelivered &&
      !isWorkerHandoffStreamEvent(event) &&
      !isBudgetApprovalStreamEvent(event);

    const next = new Map(this.turns);
    if (event.terminal) {
      if (existing.mode === "background") {
        next.set(event.turn_id, {
          ...existing,
          phase: preserveHandoff ? "worker_handoff" : this.phaseFromEvent(event),
          streamAttached: true,
          terminal: false,
        });
      } else if (this.shouldSettleTurnFromStream(event.turn_id)) {
        next.delete(event.turn_id);
      } else {
        next.set(event.turn_id, {
          ...existing,
          phase: preserveHandoff ? "worker_handoff" : this.phaseFromEvent(event),
          streamAttached: true,
          terminal: false,
        });
      }
    } else {
      next.set(event.turn_id, {
        ...existing,
        phase: preserveHandoff ? "worker_handoff" : this.phaseFromEvent(event),
        streamAttached: true,
        terminal: false,
      });
    }
    this.turns = next;
  }

  private resolveStatusLine(
    event: InteractiveTurnStreamEvent,
    current: string | null | undefined,
  ): string | null {
    if (event.message?.trim()) {
      return operatorStreamStatusLine(event, settings.showEngineDetailsInChat);
    }
    if (!settings.showEngineDetailsInChat && isEngineTelemetryText(current)) {
      return null;
    }
    return current ?? null;
  }

  private phaseFromEvent(event: InteractiveTurnStreamEvent): string {
    if (isWorkerHandoffStreamEvent(event)) return "worker_handoff";
    if (isBudgetApprovalStreamEvent(event)) return "budget_blocked";
    if (event.terminal) return "done";
    return event.phase || "streaming";
  }
}

function normalizeRole(role: string): ChatMessage["role"] {
  if (role === "user" || role === "assistant" || role === "system") {
    return role;
  }
  return "assistant";
}

function mapTurns(
  turns: SessionHistoryResponse["turns"],
  options?: {
    lane?: ChatMessage["lane"];
    askJobId?: string | null;
    sessionId?: string;
  },
): ChatMessage[] {
  const lane = options?.lane ?? "chat";
  const askJobId = options?.askJobId ?? null;
  const sessionId = options?.sessionId?.trim() || "session";
  return turns.map((turn, index) => ({
    id: `${sessionId}:${turn.timestamp}:${turn.role}:${index}`,
    role: normalizeRole(turn.role),
    content: turn.content,
    lane,
    askJobId,
    turnIndex: index + 1,
    answerState: turn.answer_state ?? null,
    tools: turn.tool_names?.length ? turn.tool_names : undefined,
    toolRuns: toolRunsFromParts(turn.parts ?? null),
    uiArtifacts: uiArtifactsFromParts(turn.parts ?? null),
    reasoning: reasoningFromParts(turn.parts ?? null),
    statusLine:
      turn.role === "assistant" ? progressFromParts(turn.parts ?? null) : null,
    mediaAttachments: userMediaFromParts(turn.parts ?? null),
  }));
}

function loadPinnedIds(): string[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(PINS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((id) => typeof id === "string") : [];
  } catch {
    return [];
  }
}

function loadSessionId(): string {
  if (typeof localStorage === "undefined") return "medousa-home";
  const existing = localStorage.getItem(SESSION_KEY);
  if (existing) return existing;
  const id = "medousa-home";
  localStorage.setItem(SESSION_KEY, id);
  return id;
}

export const chat = new ChatStore();
