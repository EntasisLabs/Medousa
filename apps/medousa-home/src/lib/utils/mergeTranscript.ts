import type { ChatMessage } from "$lib/types/chat";

/** Merge daemon history with local bubbles, preserving in-flight stream state. */
export function mergeTranscript(
  local: ChatMessage[],
  daemon: ChatMessage[],
): ChatMessage[] {
  if (local.length === 0) return daemon;
  if (daemon.length === 0) return dedupeAssistantTurns(local);

  const localTurnIds = new Set(
    local.map((message) => message.turnId).filter((id): id is string => Boolean(id?.trim())),
  );
  const daemonTurnIds = new Set(
    daemon.map((message) => message.turnId).filter((id): id is string => Boolean(id?.trim())),
  );

  const merged = [...local];
  for (const message of daemon) {
    if (message.turnId && localTurnIds.has(message.turnId)) {
      continue;
    }
    if (!message.turnId && message.role === "assistant" && message.content.trim()) {
      const duplicate = merged.some(
        (existing) =>
          existing.role === "assistant" &&
          existing.content.trim() === message.content.trim(),
      );
      if (duplicate) continue;
    }
    merged.push(message);
  }

  if (daemonTurnIds.size === 0) {
    return dedupeAssistantTurns(merged);
  }

  return dedupeAssistantTurns(merged);
}

function dedupeAssistantTurns(messages: ChatMessage[]): ChatMessage[] {
  const seenTurns = new Map<string, ChatMessage>();
  const output: ChatMessage[] = [];

  for (const message of messages) {
    if (message.role !== "assistant" || !message.turnId?.trim()) {
      output.push(message);
      continue;
    }

    const turnId = message.turnId.trim();
    const existing = seenTurns.get(turnId);
    if (!existing) {
      seenTurns.set(turnId, message);
      output.push(message);
      continue;
    }

    const keep = pickPreferredAssistantBubble(existing, message);
    if (keep.id === message.id) {
      const idx = output.findIndex((item) => item.id === existing.id);
      if (idx >= 0) {
        output[idx] = message;
      }
      seenTurns.set(turnId, message);
    }
  }

  return output;
}

function pickPreferredAssistantBubble(
  left: ChatMessage,
  right: ChatMessage,
): ChatMessage {
  if (left.streaming && !right.streaming) return left;
  if (right.streaming && !left.streaming) return right;
  if (left.content.trim().length !== right.content.trim().length) {
    return left.content.trim().length >= right.content.trim().length ? left : right;
  }
  return right;
}
