import {
  cancelActiveSessionTurn,
  getActiveSessionTurn,
  getSessionHistory,
  listSessionTurns,
  listSessions,
  startInteractiveStream,
  stopInteractiveStream,
} from "$lib/daemon";
import type {
  ChatMessage,
  InteractiveTurnStreamEvent,
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
import { reasoningFromParts, toolRunsFromParts } from "$lib/types/turnParts";
import { formatSessionLabel } from "$lib/utils/formatSession";
import { resolveTurnContent } from "$lib/utils/resolveTurnContent";

const SESSION_KEY = "medousa-home-session-id";
const PINS_KEY = "medousa-home-pinned-sessions";

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
  /** Turn worker cards linked to chat handoff bubbles (Tier 3). */
  workers = $state<Map<string, WorkerLink>>(new Map());
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
    this.workers = new Map();
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
    this.workers = new Map();
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
    this.workers = new Map();
    this.backgroundActivity = 0;
  }

  /** Workspace/worker or budget card settled — drop one background pulse unit. */
  noteBackgroundSettled(count = 1) {
    this.backgroundActivity = Math.max(0, this.backgroundActivity - count);
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
      sessionId: this.sessionId,
    });
  }

  onWorkerCardDetail(
    detail: WorkCardDetail,
    column: string,
    previousColumn: string | undefined,
  ) {
    if (detail.kind !== "turn_worker") return;
    const sessionId = detail.session_id?.trim();
    if (!sessionId || sessionId !== this.sessionId) return;

    const workId = detail.work_id?.trim() || detail.card.id;
    const parentTurnId = detail.correlation_id?.trim() || null;
    const messageId = parentTurnId ? this.messageIdForTurn(parentTurnId) : null;
    this.linkWorker({ workId, parentTurnId, messageId, sessionId });

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
      if (detail.session_id?.trim() !== this.sessionId) continue;
      this.onWorkerCardDetail(detail, card.column, undefined);
      const workId = detail.work_id?.trim() || card.id;
      const link = this.workers.get(workId);
      if (link && !link.synthesisDelivered && card.column === "done") {
        await this.deliverWorkerSynthesis(workId, detail);
      }
    }
  }

  private noteWorkerSynthesizing(workId: string) {
    const link = this.workers.get(workId);
    if (!link) return;
    this.finalizeWorkerHandoffBubble(link.messageId);
    this.ensureWorkerFollowUpBubble(workId, link.parentTurnId, {
      statusLine: "Synthesizing answer…",
      streaming: true,
    });
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
      const existing = this.messages.find((m) => m.id === link.synthesisMessageId);
      if (existing) return link.synthesisMessageId;
    }

    const id = crypto.randomUUID();
    this.messages = [
      ...this.messages,
      {
        id,
        role: "assistant",
        content: "",
        streaming: options?.streaming ?? true,
        turnId,
        statusLine: options?.statusLine ?? null,
      },
    ];

    if (link) {
      const nextWorkers = new Map(this.workers);
      nextWorkers.set(workId, { ...link, synthesisMessageId: id });
      this.workers = nextWorkers;
    }

    if (turnId) {
      const turn = this.turns.get(turnId);
      if (turn) {
        const nextTurns = new Map(this.turns);
        nextTurns.set(turnId, { ...turn, messageId: id });
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

  private async deliverWorkerSynthesis(workId: string, detail?: WorkCardDetail) {
    const link = this.workers.get(workId);
    if (!link || link.synthesisDelivered) return;

    let content = detail?.result_excerpt?.trim();
    if (!content && detail?.error?.trim()) {
      content = detail.error.trim();
    }
    if (!content) {
      const handoffContent = link.messageId
        ? this.messages.find((message) => message.id === link.messageId)?.content
        : null;
      content =
        (await this.fetchLatestAssistantTurn(link.sessionId, handoffContent)) ?? undefined;
    }
    if (!content) return;

    if (this.hasFollowUpSynthesis(link.messageId, content)) {
      this.finalizeWorkerHandoffBubble(link.messageId);
      this.markWorkerSynthesisDelivered(workId);
      return;
    }

    this.finalizeWorkerHandoffBubble(link.messageId);

    if (link.synthesisMessageId) {
      const idx = this.messages.findIndex((m) => m.id === link.synthesisMessageId);
      if (idx >= 0) {
        this.messages = [
          ...this.messages.slice(0, idx),
          {
            ...this.messages[idx],
            content,
            streaming: false,
            phase: null,
            statusLine: null,
            tools: detail?.tool_names?.length ? [...detail.tool_names] : this.messages[idx].tools,
          },
          ...this.messages.slice(idx + 1),
        ];
      } else {
        this.appendWorkerSynthesisMessage(workId, link.parentTurnId, content, detail?.tool_names);
      }
    } else {
      this.appendWorkerSynthesisMessage(workId, link.parentTurnId, content, detail?.tool_names);
    }

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

  private markWorkerSynthesisDeliveredForTurn(turnId: string) {
    for (const [workId, link] of this.workers) {
      if (link.parentTurnId === turnId && !link.synthesisDelivered) {
        this.markWorkerSynthesisDelivered(workId);
        break;
      }
    }
  }

  private appendWorkerSynthesisMessage(
    workId: string,
    parentTurnId: string | null,
    content: string,
    toolNames?: string[] | null,
  ) {
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
    const link = this.workers.get(workId);
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
    this.syncTurnFromEvent(event);

    if (event.event_type === "tool_started" || event.event_type === "tool_finished") {
      const messageId = this.messageIdForTurn(event.turn_id);
      if (messageId) {
        this.applyToolStreamEvent(messageId, event);
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
    }
  }

  private messageIdForTurn(turnId: string): string | null {
    const turn = this.turns.get(turnId);
    if (turn?.messageId) return turn.messageId;
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink?.synthesisMessageId) return workerLink.synthesisMessageId;
    if (turn?.phase === "worker_handoff" || turn?.phase === "worker_ack") {
      return null;
    }
    return (
      this.messages.find(
        (message) => message.turnId === turnId && message.role === "assistant",
      )?.id ?? null
    );
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
      const statusLine = event.message?.trim() || current.statusLine;
      if (!content.trim() && statusLine) {
        content = statusLine;
      }
      const next: ChatMessage = {
        ...current,
        content,
        phase: "tool_loop",
        statusLine: statusLine || current.statusLine,
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

    if (event.event_type === "scratch_reset") {
      const next: ChatMessage = {
        ...current,
        content: "",
        phase: "streaming",
      };
      this.messages = [
        ...this.messages.slice(0, idx),
        next,
        ...this.messages.slice(idx + 1),
      ];
      return;
    }

    if (event.content_delta) {
      content += event.content_delta;
    } else if (event.final_text) {
      const terminal =
        event.terminal ||
        event.event_type === "final" ||
        event.event_type === "needs_input" ||
        event.event_type === "error";
      content = resolveTurnContent(current.content, event.final_text, terminal);
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
      this.markWorkerSynthesisDeliveredForTurn(event.turn_id);
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
    if (turn?.phase === "worker_handoff") {
      const workerLink = this.workerLinkForTurn(event.turn_id);
      if (workerLink) {
        const nextWorkers = new Map(this.workers);
        nextWorkers.set(workerLink.workId, {
          ...workerLink,
          synthesisMessageId: id,
        });
        this.workers = nextWorkers;
      }
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
      this.markWorkerSynthesisDeliveredForTurn(event.turn_id);
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
      const ackText =
        current.content.trim() ||
        event.final_text?.trim() ||
        statusLine;
      this.messages = [
        ...this.messages.slice(0, idx),
        {
          ...current,
          streaming: false,
          phase: phase === "worker_ack" ? null : phase,
          statusLine: phase === "worker_ack" ? null : statusLine,
          content: ackText,
        },
        ...this.messages.slice(idx + 1),
      ];
    }

    const turn = this.turns.get(event.turn_id);
    if (turn) {
      const next = new Map(this.turns);
      next.set(event.turn_id, {
        ...turn,
        phase: phase === "worker_ack" ? "worker_handoff" : phase,
        messageId: phase === "worker_ack" ? null : messageId,
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
      this.linkWorkerFromStream(event, messageId);
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
    toolRuns: toolRunsFromParts(turn.parts ?? null),
    reasoning: reasoningFromParts(turn.parts ?? null),
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
