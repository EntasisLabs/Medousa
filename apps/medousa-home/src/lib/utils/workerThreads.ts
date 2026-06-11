import type { ChatMessage } from "$lib/types/chat";

export interface WorkerThread {
  workId: string;
  messages: ChatMessage[];
  active: boolean;
  intent: string;
  statusLine: string;
}

export function isWorkerLaneMessage(message: ChatMessage): boolean {
  return message.lane === "worker";
}

export function groupWorkerThreads(messages: ChatMessage[]): WorkerThread[] {
  const byWork = new Map<string, ChatMessage[]>();

  for (const message of messages) {
    if (message.lane !== "worker") continue;
    const workId =
      message.workId?.trim() || message.turnId?.trim() || message.id;
    const bucket = byWork.get(workId) ?? [];
    bucket.push(message);
    byWork.set(workId, bucket);
  }

  return [...byWork.entries()]
    .map(([workId, threadMessages]) => {
      const active = threadMessages.some((message) => message.streaming);
      const statusLine =
        threadMessages.find((message) => message.statusLine?.trim())
          ?.statusLine?.trim() ??
        (active ? "Working in background…" : "Settled");
      return {
        workId,
        messages: threadMessages,
        active,
        intent: "worker",
        statusLine,
      };
    })
    .sort((left, right) => {
      const leftLatest = left.messages.at(-1)?.id ?? "";
      const rightLatest = right.messages.at(-1)?.id ?? "";
      return rightLatest.localeCompare(leftLatest);
    });
}

export function workerStatusLineForColumn(column: string): string {
  switch (column) {
    case "running":
      return "Running tools…";
    case "wrapping_up":
      return "Pulling that together…";
    case "done":
      return "Complete";
    case "blocked":
      return "Needs attention";
    default:
      return "Working in background…";
  }
}
