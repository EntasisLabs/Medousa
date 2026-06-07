import type { ChatMessage, InteractiveTurnStreamEvent } from "$lib/types/chat";

const SESSION_KEY = "medousa-home-session-id";

export class ChatStore {
  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state("");
  isStreaming = $state(false);
  streamError = $state<string | null>(null);
  private assistantId: string | null = null;

  resetSession() {
    const id = `medousa-home-${crypto.randomUUID()}`;
    localStorage.setItem(SESSION_KEY, id);
    this.sessionId = id;
    this.messages = [];
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

    const next = { ...current, content };
    this.messages = [
      ...this.messages.slice(0, idx),
      next,
      ...this.messages.slice(idx + 1),
    ];

    if (event.terminal) {
      this.finishStream();
    }
  }

  finishStream() {
    if (!this.assistantId) return;
    const idx = this.messages.findIndex((m) => m.id === this.assistantId);
    if (idx >= 0) {
      const next = { ...this.messages[idx], streaming: false };
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

function loadSessionId(): string {
  if (typeof localStorage === "undefined") return "medousa-home";
  const existing = localStorage.getItem(SESSION_KEY);
  if (existing) return existing;
  const id = "medousa-home";
  localStorage.setItem(SESSION_KEY, id);
  return id;
}

export const chat = new ChatStore();
