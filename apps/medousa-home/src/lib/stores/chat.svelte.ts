import { getSessionHistory, listSessions } from "$lib/daemon";
import type { ChatMessage, InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { SessionHistoryResponse, SessionSummary } from "$lib/types/session";
import { formatSessionLabel } from "$lib/utils/formatSession";

const SESSION_KEY = "medousa-home-session-id";
const PINS_KEY = "medousa-home-pinned-sessions";

export class ChatStore {
  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state("");
  /** Live SSE attached to the current assistant bubble — blocks composer. */
  liveStreamActive = $state(false);
  /** Worker handoffs and operator pauses still running outside the live stream. */
  backgroundActivity = $state(0);
  streamError = $state<string | null>(null);
  sessions = $state<SessionSummary[]>([]);
  sessionsError = $state<string | null>(null);
  pinnedIds = $state<string[]>(loadPinnedIds());
  historyLoading = $state(false);
  /** Brief banner after reloading turns from the Mac daemon (e.g. after WebView refresh). */
  historyNotice = $state<string | null>(null);
  private assistantId: string | null = null;
  /** Bumps when the local transcript changes; stale daemon reloads must not overwrite it. */
  private transcriptEpoch = 0;

  /** True while the composer must wait on the active live stream bubble. */
  get composerBlocked(): boolean {
    return this.liveStreamActive;
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
    if (this.liveStreamActive) return;
    this.transcriptEpoch += 1;
    const id = `medousa-home-${crypto.randomUUID()}`;
    localStorage.setItem(SESSION_KEY, id);
    this.sessionId = id;
    this.messages = [];
    this.streamError = null;
    this.historyNotice = null;
    this.backgroundActivity = 0;
    await this.refreshSessions();
  }

  /** Pull transcript from daemon when the UI remounted empty (startup / reconnect). */
  async ensureSessionHydrated(options?: { notice?: boolean }) {
    if (this.liveStreamActive || this.historyLoading || this.messages.length > 0) {
      return;
    }
    await this.reloadCurrentSession(options);
  }

  /** Fetch current session history from the Mac daemon (survives WebView refresh). */
  async reloadCurrentSession(options?: { notice?: boolean }) {
    if (this.liveStreamActive) return;
    const sessionId = this.sessionId.trim();
    if (!sessionId) return;

    const epoch = this.transcriptEpoch;
    this.historyLoading = true;
    this.streamError = null;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch || this.liveStreamActive) return;
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
    if (this.liveStreamActive) return;
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
    this.historyLoading = true;
    const epoch = this.transcriptEpoch;
    try {
      const history = await getSessionHistory(sessionId);
      if (epoch !== this.transcriptEpoch) return;
      this.messages = mapTurns(history.turns);
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

  /** Workspace/worker or budget card settled — drop one background pulse unit. */
  noteBackgroundSettled(count = 1) {
    this.backgroundActivity = Math.max(0, this.backgroundActivity - count);
  }

  beginUserMessage(content: string) {
    this.transcriptEpoch += 1;
    this.historyNotice = null;
    this.messages = [
      ...this.messages,
      { id: crypto.randomUUID(), role: "user", content },
      {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        streaming: true,
      },
    ];
    this.assistantId = this.messages.at(-1)?.id ?? null;
    this.liveStreamActive = true;
    this.streamError = null;
  }

  applyStreamEvent(event: InteractiveTurnStreamEvent) {
    if (!this.assistantId) {
      if (event.content_delta || event.final_text || event.event_type === "content_delta") {
        this.attachOrphanStream(event);
        return;
      }
      if (event.terminal) {
        this.liveStreamActive = false;
        if (this.backgroundActivity > 0) {
          this.backgroundActivity -= 1;
        }
        void this.refreshSessions();
      }
      return;
    }

    const idx = this.messages.findIndex((m) => m.id === this.assistantId);
    if (idx < 0) {
      if (event.content_delta || event.final_text) {
        this.attachOrphanStream(event);
        return;
      }
      if (event.terminal) {
        this.assistantId = null;
        this.liveStreamActive = false;
        if (this.backgroundActivity > 0) {
          this.backgroundActivity -= 1;
        }
        void this.refreshSessions();
      }
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
      this.releaseComposerHandoff(
        "worker_ack",
        event.message?.trim() || "Background worker started",
      );
      void this.refreshSessions();
      return;
    }

    if (event.event_type === "budget_approval") {
      this.releaseComposerHandoff(
        "budget_approval",
        event.message?.trim() || "Waiting for operator approval",
      );
      return;
    }

    if (event.terminal) {
      if (this.backgroundActivity > 0) {
        this.backgroundActivity -= 1;
      }
      this.finishStream();
      void this.refreshSessions();
    }
  }

  /** Resume stream after handoff (e.g. budget approved) with no active assistant bubble. */
  private attachOrphanStream(event: InteractiveTurnStreamEvent) {
    const content = event.final_text ?? event.content_delta ?? "";
    if (!content && !event.terminal && event.event_type !== "budget_approval") {
      return;
    }

    const id = crypto.randomUUID();
    this.messages = [
      ...this.messages,
      {
        id,
        role: "assistant",
        content,
        streaming: !event.terminal,
        phase: event.phase || null,
        statusLine: event.message?.trim() || null,
        tools: event.tool_names?.length ? [...event.tool_names] : undefined,
      },
    ];
    this.assistantId = event.terminal ? null : id;
    this.liveStreamActive = !event.terminal;

    if (event.event_type === "worker_ack") {
      this.releaseComposerHandoff(
        "worker_ack",
        event.message?.trim() || "Background worker started",
      );
      void this.refreshSessions();
      return;
    }

    if (event.event_type === "budget_approval") {
      this.releaseComposerHandoff(
        "budget_approval",
        event.message?.trim() || "Waiting for operator approval",
      );
      return;
    }

    if (event.terminal) {
      if (this.backgroundActivity > 0) {
        this.backgroundActivity -= 1;
      }
      this.finishStream();
      void this.refreshSessions();
    }
  }

  private releaseComposerHandoff(
    phase: "worker_ack" | "budget_approval",
    statusLine: string,
  ) {
    if (this.assistantId) {
      const idx = this.messages.findIndex((m) => m.id === this.assistantId);
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
    }
    this.assistantId = null;
    this.liveStreamActive = false;
    this.backgroundActivity += 1;
  }

  finishStream() {
    if (!this.assistantId) {
      this.liveStreamActive = false;
      return;
    }
    const idx = this.messages.findIndex((m) => m.id === this.assistantId);
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
    this.assistantId = null;
    this.liveStreamActive = false;
  }

  setError(message: string) {
    this.streamError = message;
    this.finishStream();
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
