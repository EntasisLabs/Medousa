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
    return this.pages.flatMap(page => page.cursor === null ? live : page.messages);
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
