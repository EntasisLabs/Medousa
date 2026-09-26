import type { ChatMessage } from "$lib/types/chat";

/** Fold durable attachment entries into their exact committed assistant entry. */
export function attachLiveTranscripts(messages: ChatMessage[]): ChatMessage[] {
  const matches = (message: ChatMessage, target: string) => message.role === "assistant" && (target.startsWith("execution:") ? message.turnId === target.slice(10) : message.id.endsWith(`:${target}`));
  const result = messages.filter((message) => !message.segments?.some((segment) => segment.kind === "handoff" && segment.handoffKind === "live_transcript" && segment.workId && messages.some((target) => matches(target, segment.workId!))));
  for (const message of messages) {
    for (const segment of message.segments ?? []) {
      if (segment.kind !== "handoff" || segment.handoffKind !== "live_transcript") continue;
      const target = (segment.workId ? result.find((item) => matches(item, segment.workId!)) : null) ?? result.find((item) => item.id === message.id);
      if (!target) continue;
      try {
        const rows: unknown = JSON.parse(segment.text);
        if (!Array.isArray(rows) || !rows.every((row) => row && ["user", "assistant"].includes(row.role) && typeof row.text === "string")) continue;
        target.liveTranscripts = [...(target.liveTranscripts ?? []), { id: message.id, rows }];
        if (target.id === message.id) {
          target.segments = undefined;
          if (!target.content.trim()) target.content = "Voice transcript";
        }
      } catch { /* Corrupt attachment data must not break chat rendering. */ }
    }
  }
  return result;
}
