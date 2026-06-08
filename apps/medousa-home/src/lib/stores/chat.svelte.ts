import {
  cancelActiveSessionTurn,
  getActiveSessionTurn,
  getSessionHistory,
  listSessionTurns,
  listSessions,
  startInteractiveStream,
  stopInteractiveStream,
} from "$lib/daemon";
import type { ChatMessage, InteractiveTurnStreamEvent, TurnTicketState } from "$lib/types/chat";
import type {
  SessionHistoryResponse,
  SessionSummary,
  TurnTicketRecord,
  TurnTicketResponse,
} from "$lib/types/session";
import { formatSessionLabel } from "$lib/utils/formatSession";

const SESSION_KEY = "medousa-home-session-id";
const PINS_KEY = "medousa-home-pinned-sessions";

export class ChatStore {
  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state("");
  /** Worker handoffs and operator pauses still running outside the live stream. */
  backgroundActivity = $state(0);
  streamError = $state<string | null>(null);
  sessions = $state<SessionSummary[]>([]);
  sessionsError = $state<string | null>(null);
  pinnedIds = $state<string[]>(loadPinnedIds());
  historyLoading = $state(false);
  /** Brief banner after reloading turns from the Mac daemon (e.g. after WebView refresh). */
  historyNotice = $state<string | null>(null);
  /** Daemon turn id for the live interactive stream, if any. */
  activeTurnId = $state<string | null>(null);
  /** Turn-centric state keyed by daemon turn id. */
  turns = $state<Map<string, TurnTicketState>>(new Map());
  private assistantId: string | null = null;
  /** Bumps when the local transcript changes; stale daemon reloads must not overwrite it. */
  private transcriptEpoch = 0;

  /** True while the composer must wait — Tier 2c: always open. */
  get composerBlocked(): boolean {
    return false;
  }

  /** Interactive turn still streaming tokens (UI pulse only). */
  get liveStreamActive(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (turn.phase === "worker_handoff" || turn.phase === "budget_blocked") {
        continue;
      }
      return true;
    }
    return false;
  }

  /** Non-terminal interactive ticket in flight (fork policy gate). */
  hasLiveInteractiveTurn(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (turn.phase === "worker_handoff" || turn.phase === "budget_blocked") {
        continue;
      }
      return true;
    }
    return false;
  }

  /** Live stream and/or background worker / approval work in flight. */
  get hasTurnActivity(): boolean {
    return this.liveStreamActive || this.backgroundActivity > 0;
  }

  /** Back-compat alias for live stream only (not background handoffs). */
  get isStreaming(): boolean {
    return this.liveStreamActive;
  }

  isPinned(sessionId: string): boolean {
    return this.pinnedIds.includes(sessionId);
  }

  currentSessionLabel(): string {
    const match = this.sessions.find((session) => session.session_id === this.sessionId);
    if (match) return formatSessionLabel(match);

    const firstUser = this.messages.find((message) => message.role === "user");
    if (firstUser?.content.trim()) {
      const line = firstUser.content.trim().split("\n")[0];
      return line.length > 48 ? `${line.slice(0, 47)}…` : line;
    }

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

  async refreshSessions() {
    this.sessionsError = null;
    try {
      const response = await listSessions(50);
      this.sessions = response.sessions;
    } catch (err) {
      this.sessionsError = err instanceof Error ? err.message : String(err);
    }
  }

  async newSession() {
    this.transcriptEpoch += 1;
    const id = `medousa-home-${crypto.randomUUID()}`;
    localStorage.setItem(SESSION_KEY, id);
    this.sessionId = id;
    this.messages = [];
    this.streamError = null;
    this.historyNotice = null;
    this.backgroundActivity = 0;
    this.activeTurnId = null;
    this.turns = new Map();
    await this.refreshSessions();
  }

  /** Pull transcript from daemon when the UI remounted empty (startup / reconnect). */
  async ensureSessionHydrated(options?: { notice?: boolean }) {
    if (this.historyLoading || this.messages.length > 0) {
      return;
    }
    await this.reloadCurrentSession(options);
  }

  /** Fetch current session history from the Mac daemon (survives WebView refresh). */
  async reloadCurrentSession(options?: { notice?: boolean }) {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    const epoch = this.transcriptEpoch;
    this.historyLoading = true;
    this.streamError = null;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;
      this.messages = mapTurns(history.turns);
      if (options?.notice !== false && history.turns.length > 0) {
        const count = history.turns.length;
        this.historyNotice = `Restored ${count} turn${count === 1 ? "" : "s"} from Mac`;
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
    this.transcriptEpoch += 1;
    this.sessionId = sessionId;
    localStorage.setItem(SESSION_KEY, sessionId);
    this.streamError = null;
    this.historyNotice = null;
    this.messages = [];
    this.backgroundActivity = 0;
    this.activeTurnId = null;
    this.turns = new Map();
    this.historyLoading = true;
    const epoch = this.transcriptEpoch;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;
      this.messages = mapTurns(history.turns);
      await this.tryReattachActiveTurn();
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
  beginTurn(userContent: string, ticket: TurnTicketResponse) {
    this.transcriptEpoch += 1;
    this.historyNotice = null;
    const assistantId = crypto.randomUUID();
    this.messages = [
      ...this.messages,
      { id: crypto.randomUUID(), role: "user", content: userContent },
      {
        id: assistantId,
        role: "assistant",
        content: "",
        streaming: true,
        turnId: ticket.turn_id,
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

  /**
   * Reattach SSE listeners for all active session turns after refresh/reconnect.
   */
  async tryReattachActiveTurn(): Promise<boolean> {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return false;

    try {
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

      let attached = false;
      for (const record of response.turns) {
        attached = true;
        let messageId = this.messages.find(
          (m) => m.turnId === record.turn_id && m.role === "assistant",
        )?.id;

        if (!messageId && !record.composer_handoff) {
          messageId = crypto.randomUUID();
          this.messages = [
            ...this.messages,
            {
              id: messageId,
              role: "assistant",
              content: "",
              streaming: true,
              turnId: record.turn_id,
            },
          ];
          if (record.mode === "interactive") {
            this.assistantId = messageId;
          }
        }

        this.registerTurnFromRecord(record, messageId ?? null);
        if (record.composer_handoff && record.mode === "interactive") {
          this.backgroundActivity = Math.max(this.backgroundActivity, 1);
        } else if (record.mode === "background") {
          this.backgroundActivity = Math.max(this.backgroundActivity, 1);
        }

        await startInteractiveStream(record.stream_url);
      }

      return attached;
    } catch {
      return false;
    }
  }

  /** Cancel the daemon-side turn and detach the local SSE listener. */
  async cancelActiveTurn(): Promise<void> {
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    try {
      await cancelActiveSessionTurn(sessionId);
    } catch {
      // Best-effort — still stop the local listener below.
    }

    await stopInteractiveStream();
    this.activeTurnId = null;
    this.assistantId = null;
    this.turns = new Map();
    this.backgroundActivity = 0;
  }

  /** Workspace/worker or budget card settled — drop one background pulse unit. */
  noteBackgroundSettled(count = 1) {
    this.backgroundActivity = Math.max(0, this.backgroundActivity - count);
  }

  applyStreamEvent(event: InteractiveTurnStreamEvent) {
    this.syncTurnFromEvent(event);

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
    }
  }

  private messageIdForTurn(turnId: string): string | null {
    const turn = this.turns.get(turnId);
    if (turn?.messageId) return turn.messageId;
    return (
      this.messages.find(
        (m) => m.turnId === turnId && m.role === "assistant",
      )?.id ?? null
    );
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

    if (event.content_delta) {
      content += event.content_delta;
    } else if (event.final_text) {
      content = event.final_text;
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
      statusLine: event.message?.trim() || current.statusLine,
      tools: tools.length > 0 ? tools : current.tools,
      reasoning: reasoning || current.reasoning,
    };
    this.messages = [
      ...this.messages.slice(0, idx),
      next,
      ...this.messages.slice(idx + 1),
    ];

    if (event.event_type === "worker_ack") {
      this.releaseComposerHandoff(messageId, "worker_ack", event);
      void this.refreshSessions();
      return;
    }

    if (event.event_type === "budget_approval") {
      this.releaseComposerHandoff(messageId, "budget_approval", event);
      return;
    }

    if (event.terminal) {
      this.finishMessage(messageId);
      this.noteTurnTerminal(event);
      void this.refreshSessions();
    }
  }

  private noteTurnTerminal(event: InteractiveTurnStreamEvent) {
    const turn = this.turns.get(event.turn_id);
    if (turn?.mode === "background" || this.backgroundActivity > 0) {
      this.backgroundActivity = Math.max(0, this.backgroundActivity - 1);
    }
    if (this.activeTurnId === event.turn_id) {
      this.activeTurnId = null;
    }
    if (this.assistantId && turn?.messageId === this.assistantId) {
      this.assistantId = null;
    }
  }

  /** Resume stream after handoff (e.g. budget approved) with no active assistant bubble. */
  private attachOrphanStream(event: InteractiveTurnStreamEvent) {
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
        statusLine: event.message?.trim() || null,
        tools: event.tool_names?.length ? [...event.tool_names] : undefined,
      },
    ];
    if (turn) {
      const next = new Map(this.turns);
      next.set(event.turn_id, { ...turn, messageId: id });
      this.turns = next;
    }
    if (turn?.mode === "interactive" && !event.terminal) {
      this.assistantId = id;
    }

    if (event.event_type === "worker_ack") {
      this.releaseComposerHandoff(id, "worker_ack", event);
      void this.refreshSessions();
      return;
    }

    if (event.event_type === "budget_approval") {
      this.releaseComposerHandoff(id, "budget_approval", event);
      return;
    }

    if (event.terminal) {
      this.finishMessage(id);
      this.noteTurnTerminal(event);
      void this.refreshSessions();
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

    const idx = this.messages.findIndex((m) => m.id === messageId);
    if (idx >= 0) {
      const current = this.messages[idx];
      this.messages = [
        ...this.messages.slice(0, idx),
        {
          ...current,
          streaming: false,
          phase,
          statusLine,
          content: current.content.trim() || statusLine,
        },
        ...this.messages.slice(idx + 1),
      ];
    }

    const turn = this.turns.get(event.turn_id);
    if (turn) {
      const next = new Map(this.turns);
      next.set(event.turn_id, {
        ...turn,
        phase,
        messageId,
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
    this.streamError = message;
    if (this.assistantId) {
      this.finishMessage(this.assistantId);
    }
  }

  private syncTurnFromEvent(event: InteractiveTurnStreamEvent) {
    let existing = this.turns.get(event.turn_id);
    if (!existing && (event.content_delta || event.final_text || event.terminal)) {
      existing = {
        turnId: event.turn_id,
        mode: "background",
        phase: event.phase,
        messageId: null,
        streamAttached: true,
        terminal: false,
        workspaceCardId: null,
      };
    }
    if (!existing) return;

    const next = new Map(this.turns);
    if (event.terminal) {
      next.delete(event.turn_id);
    } else {
      next.set(event.turn_id, {
        ...existing,
        phase: this.phaseFromEvent(event),
        streamAttached: true,
        terminal: false,
      });
    }
    this.turns = next;
  }

  private phaseFromEvent(event: InteractiveTurnStreamEvent): string {
    if (event.event_type === "worker_ack") return "worker_handoff";
    if (event.event_type === "budget_approval") return "budget_blocked";
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

function mapTurns(turns: SessionHistoryResponse["turns"]): ChatMessage[] {
  return turns.map((turn) => ({
    id: crypto.randomUUID(),
    role: normalizeRole(turn.role),
    content: turn.content,
    answerState: turn.answer_state ?? null,
    tools: turn.tool_names?.length ? turn.tool_names : undefined,
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
