import type { ChatMessage } from "$lib/types/chat";

export interface AskThread {
  jobId: string;
  messages: ChatMessage[];
  active: boolean;
  promptPreview: string;
}

export function isChatLaneMessage(message: ChatMessage): boolean {
  return (message.lane ?? "chat") === "chat";
}

export function groupAskThreads(messages: ChatMessage[]): AskThread[] {
  const byJob = new Map<string, ChatMessage[]>();

  for (const message of messages) {
    if (message.lane !== "ask") continue;
    const jobId = message.askJobId?.trim() || message.turnId?.trim() || message.id;
    const bucket = byJob.get(jobId) ?? [];
    bucket.push(message);
    byJob.set(jobId, bucket);
  }

  return [...byJob.entries()]
    .map(([jobId, threadMessages]) => {
      const userLine = threadMessages.find((message) => message.role === "user");
      const preview = userLine?.content.trim().split("\n")[0] ?? "Ask";
      return {
        jobId,
        messages: threadMessages,
        active: threadMessages.some((message) => message.streaming),
        promptPreview: preview.length > 72 ? `${preview.slice(0, 71)}…` : preview,
      };
    })
    .sort((left, right) => {
      const leftUser = left.messages.find((message) => message.role === "user");
      const rightUser = right.messages.find((message) => message.role === "user");
      return (rightUser?.id ?? "").localeCompare(leftUser?.id ?? "");
    });
}
