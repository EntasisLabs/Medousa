import { getSessionHistory, listSessions } from "$lib/daemon";
import type { ChatMessage, InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { SessionSummary } from "$lib/types/session";
import { formatSessionLabel } from "$lib/utils/formatSession";

const SESSION_KEY = "medousa-home-session-id";
const PINS_KEY = "medousa-home-pinned-sessions";

export class ChatStore {
  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state("");
  isStreaming = $state(false);
  streamError = $state<string | null>(null);
  sessions = $state<SessionSummary[]>([]);
  sessionsError = $state<string | null>(null);
  pinnedIds = $state<string[]>(loadPinnedIds());
  private assistantId: string | null = null;

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
    if (this.isStreaming) return;
    const id = `medousa-home-${crypto.randomUUID()}`;
    localStorage.setItem(SESSION_KEY, id);
    this.sessionId = id;
    this.messages = [];
    this.streamError = null;
    await this.refreshSessions();
  }

  async switchSession(sessionId: string) {
    if (this.isStreaming || sessionId === this.sessionId) return;
    this.sessionId = sessionId;
    localStorage.setItem(SESSION_KEY, sessionId);
    this.streamError = null;
    this.messages = [];
    try {
      const history = await getSessionHistory(sessionId);
      this.messages = history.turns.map((turn) => ({
        id: crypto.randomUUID(),
        role: normalizeRole(turn.role),
        content: turn.content,
      }));
    } catch (err) {
      this.streamError = err instanceof Error ? err.message : String(err);
    }
  }

  beginUserMessage(content: string) {
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
    this.isStreaming = true;
    this.streamError = null;
  }

  applyStreamEvent(event: InteractiveTurnStreamEvent) {
    if (!this.assistantId) return;

    const idx = this.messages.findIndex((m) => m.id === this.assistantId);
    if (idx < 0) return;

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

    if (event.terminal) {
      this.finishStream();
      void this.refreshSessions();
    }
  }

  finishStream() {
    if (!this.assistantId) return;
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
    this.isStreaming = false;
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
