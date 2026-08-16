import type { ChatMessage, TurnTicketState } from "$lib/types/chat";

/** Per-session chat UI/stream state (kept when focus moves to another session). */
export type ChatSessionRuntime = {
  sessionId: string;
  messages: ChatMessage[];
  draft: string;
  streamError: string | null;
  historyLoading: boolean;
  sessionPristine: boolean;
  historyNotice: string | null;
  activeTurnId: string | null;
  turns: Map<string, TurnTicketState>;
  workers: Map<string, WorkerLinkSnapshot>;
  assistantId: string | null;
  transcriptEpoch: number;
  lastSeqByTurn: Map<string, number>;
  backgroundActivity: number;
};

type WorkerLinkSnapshot = {
  workId: string;
  parentTurnId: string | null;
  messageId: string | null;
  synthesisMessageId: string | null;
  sessionId: string;
  synthesisDelivered: boolean;
};

export function emptySessionRuntime(sessionId: string, draft = ""): ChatSessionRuntime {
  return {
    sessionId,
    messages: [],
    draft,
    streamError: null,
    historyLoading: false,
    sessionPristine: false,
    historyNotice: null,
    activeTurnId: null,
    turns: new Map(),
    workers: new Map(),
    assistantId: null,
    transcriptEpoch: 0,
    lastSeqByTurn: new Map(),
    backgroundActivity: 0,
  };
}

export function cloneRuntime(runtime: ChatSessionRuntime): ChatSessionRuntime {
  return {
    ...runtime,
    messages: runtime.messages.map((message) => ({ ...message })),
    turns: new Map(runtime.turns),
    workers: new Map(
      [...runtime.workers.entries()].map(([key, value]) => [key, { ...value }]),
    ),
    lastSeqByTurn: new Map(runtime.lastSeqByTurn),
  };
}
