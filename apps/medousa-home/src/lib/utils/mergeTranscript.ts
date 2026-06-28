import type { ChatMessage } from "$lib/types/chat";

/** Collapse runs of whitespace so trivial formatting drift doesn't defeat dedup. */
function normalizeForCompare(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/** Keep first occurrence — Svelte keyed `{#each}` throws on duplicate message ids. */
export function dedupeMessagesById(messages: ChatMessage[]): ChatMessage[] {
  const seen = new Set<string>();
  const output: ChatMessage[] = [];
  for (const message of messages) {
    if (seen.has(message.id)) continue;
    seen.add(message.id);
    output.push(message);
  }
  return output;
}

/** Merge daemon history with local bubbles, preserving in-flight stream state. */
export function mergeTranscript(
  local: ChatMessage[],
  daemon: ChatMessage[],
): ChatMessage[] {
  if (local.length === 0) return dedupeMessagesById(daemon);
  if (daemon.length === 0) return dedupeMessagesById(dedupeAssistantTurns(local));

  const localTurnIds = new Set(
    local.map((message) => message.turnId).filter((id): id is string => Boolean(id?.trim())),
  );

  const merged = [...local];
  for (const message of daemon) {
    if (message.turnId && localTurnIds.has(message.turnId)) {
      continue;
    }
    // Daemon history rows carry no turnId, so fall back to a whitespace-normalized
    // content compare. Exact `.trim()` matching let a streamed bubble and its
    // persisted twin diverge on minor markdown/whitespace and surface as a dup.
    if (message.role === "user") {
      const normalized = normalizeForCompare(message.content);
      const duplicateUser = merged.some(
        (existing) =>
          existing.role === "user" &&
          normalizeForCompare(existing.content) === normalized,
      );
      if (duplicateUser) continue;
    }
    if (!message.turnId && message.role === "assistant" && message.content.trim()) {
      const normalized = normalizeForCompare(message.content);
      const duplicate = merged.some(
        (existing) =>
          existing.role === "assistant" &&
          normalizeForCompare(existing.content) === normalized,
      );
      if (duplicate) continue;
    }
    merged.push(message);
  }

  return dedupeMessagesById(dedupeAssistantTurns(merged));
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
