import type { ChatMessage } from "$lib/types/chat";

export type TranscriptPage = {
  /** null is the live/latest page, read from the session rather than copied. */
  cursor: string | null;
  olderCursor: string | null;
  messages: ChatMessage[];
};

/** Retain nearby payloads and only reload coordinates for discarded newer pages. */
export class TranscriptWindow {
  pages: TranscriptPage[];
  newer: Array<string | null> = [];
  loading = false;

  constructor(olderCursor: string | null) {
    this.pages = [{ cursor: null, olderCursor, messages: [] }];
  }

  get atLatest() { return this.newer.length === 0; }
  get olderCursor() { return this.pages[0]?.olderCursor ?? null; }

  messages(live: ChatMessage[]): ChatMessage[] {
    // History pages can overlap each other and the retained live tail after
    // resume. Keep the original position but prefer the later (live) object;
    // concatenating these buffers paints the same turn multiple times.
    const output: ChatMessage[] = [];
    const byId = new Map<string, number>();
    const byTurn = new Map<string, number>();
    for (const page of this.pages) {
      for (const message of page.cursor === null ? live : page.messages) {
        const turnKey = message.role === "assistant" && message.turnId?.trim()
          ? JSON.stringify([message.turnId.trim(), message.lane ?? "chat", message.workId ?? null, message.askJobId ?? null])
          : null;
        const index = byId.get(message.id) ?? (turnKey ? byTurn.get(turnKey) : undefined);
        const position = index ?? output.length;
        if (index === undefined) output.push(message);
        else output[index] = message;
        byId.set(message.id, position);
        if (turnKey) byTurn.set(turnKey, position);
      }
    }
    return output;
  }

  prepend(page: TranscriptPage) {
    this.pages.unshift(page);
    while (this.pages.length > 3) this.newer.push(this.pages.pop()!.cursor);
  }

  append(page: TranscriptPage) {
    this.newer.pop();
    this.pages.push(page);
    while (this.pages.length > 3) this.pages.shift();
  }
}

/** Keep a short live tail plus any uncommitted/streaming turns, separate from old pages. */
export function boundedLiveTranscript(messages: ChatMessage[], keepIds = new Set<string>()): ChatMessage[] {
  if (messages.length <= 48) return messages;
  const firstKept = messages.length - 48;
  return messages.filter((message, index) => index >= firstKept || message.streaming || keepIds.has(message.id));
}
