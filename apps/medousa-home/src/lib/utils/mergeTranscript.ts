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

  const merged = [...local];
  const claimed = new Set<string>();
  for (const message of daemon) {
    const identityIndex = merged.findIndex((item) => item.role === message.role && (
      item.id === message.id || sameEntry(item, message) ||
      (message.role === "assistant" && message.turnId && item.turnId === message.turnId)
    ));
    if (identityIndex >= 0) {
      claimed.add(merged[identityIndex].id);
      merged[identityIndex] = hydrateAssistantMetadata(merged[identityIndex], message);
      continue;
    }
    // Only unmatched optimistic rows may use the legacy content fallback.
    // Durable entries with the same words are separate occurrences.
    if (message.role === "user") {
      const normalized = normalizeForCompare(message.content);
      const duplicateIndex = merged.findIndex(
        (existing) => existing.role === "user" && !existing.transcript && !claimed.has(existing.id) &&
          (!existing.createdAt || !message.createdAt || existing.createdAt === message.createdAt) &&
          normalizeForCompare(existing.content) === normalized,
      );
      if (duplicateIndex >= 0) {
        claimed.add(merged[duplicateIndex].id);
        merged[duplicateIndex] = hydrateAssistantMetadata(merged[duplicateIndex], message);
        continue;
      }
    }
    if (!message.turnId && message.role === "assistant" && message.content.trim()) {
      const normalized = normalizeForCompare(message.content);
      const duplicateIndex = merged.findIndex(
        (existing) =>
          existing.role === "assistant" && !existing.transcript && !claimed.has(existing.id) &&
          normalizeForCompare(existing.content) === normalized,
      );
      if (duplicateIndex >= 0) {
        const existing = merged[duplicateIndex];
        claimed.add(existing.id);
        merged[duplicateIndex] = hydrateAssistantMetadata(existing, message);
        continue;
      }
    }
    const coordinate = message.transcript;
    const nextIndex = coordinate ? merged.findIndex(item =>
      item.transcript?.authorityId === coordinate.authorityId &&
      item.transcript?.sessionId === coordinate.sessionId &&
      item.transcript.entrySeq > coordinate.entrySeq,
    ) : -1;
    if (nextIndex >= 0) merged.splice(nextIndex, 0, message);
    else {
      // A refreshed client may only have an active shell, not yet a durable
      // coordinate for its prompt. Recovered history precedes that live tail.
      let liveIndex = coordinate ? merged.findIndex(item => item.role === "assistant" &&
        item.streaming && !item.transcript && (item.lane == null || item.lane === "chat")) : -1;
      if (liveIndex > 0 && merged[liveIndex - 1].role === "user" && !merged[liveIndex - 1].transcript) liveIndex -= 1;
      if (liveIndex >= 0) merged.splice(liveIndex, 0, message);
      else merged.push(message);
    }
  }

  return orderBySequence(dedupeMessagesById(dedupeAssistantTurns(merged)));
}

/** Keep uncommitted tails with their durable predecessor while repairing order. */
function orderBySequence(messages: ChatMessage[]): ChatMessage[] {
  const prefix: ChatMessage[] = [];
  const bundles: ChatMessage[][] = [];
  const scopes = new Map<string, number[]>();
  for (const message of messages) {
    const coordinate = message.transcript;
    if (!coordinate) {
      (bundles.at(-1) ?? prefix).push(message);
      continue;
    }
    const key = JSON.stringify([coordinate.authorityId, coordinate.sessionId]);
    const indexes = scopes.get(key) ?? [];
    indexes.push(bundles.length);
    scopes.set(key, indexes);
    bundles.push([message]);
  }
  for (const indexes of scopes.values()) {
    const ordered = indexes.map(index => bundles[index]).sort((left, right) =>
      left[0].transcript!.entrySeq - right[0].transcript!.entrySeq);
    indexes.forEach((index, offset) => { bundles[index] = ordered[offset]; });
  }
  return [...prefix, ...bundles.flat()];
}

function sameEntry(left: ChatMessage, right: ChatMessage): boolean {
  return Boolean(left.transcript && right.transcript &&
    left.transcript.authorityId === right.transcript.authorityId &&
    left.transcript.sessionId === right.transcript.sessionId &&
    left.transcript.entryId === right.transcript.entryId);
}

function hydrateAssistantMetadata(existing: ChatMessage, message: ChatMessage): ChatMessage {
  return {
    ...existing,
    content: existing.streaming ? existing.content : message.content,
    createdAt: message.createdAt ?? existing.createdAt,
    turnIndex: message.turnIndex ?? existing.turnIndex,
    turnId: existing.turnId ?? message.turnId,
    failed: existing.streaming ? existing.failed : message.failed ?? existing.failed,
    errorLine: existing.streaming ? existing.errorLine : message.errorLine ?? existing.errorLine,
    mediaAttachments: message.mediaAttachments ?? existing.mediaAttachments,
    transcript: message.transcript ?? existing.transcript,
    liveTranscripts: message.liveTranscripts ?? existing.liveTranscripts,
    answerState: existing.streaming ? existing.answerState : message.answerState ?? existing.answerState,
    tools: existing.streaming ? existing.tools : message.tools ?? existing.tools,
    toolRuns: existing.streaming ? existing.toolRuns : message.toolRuns ?? existing.toolRuns,
    // A history snapshot can be ahead of the SSE cursor. Replacing live text
    // here makes the pending append events append that same suffix again.
    // Attachments may hydrate independently; streamed projections must stay
    // paired with the cursor that produced them until the bubble settles.
    segments: existing.streaming ? existing.segments : message.segments ?? existing.segments,
    uiArtifacts: existing.streaming ? existing.uiArtifacts : message.uiArtifacts ?? existing.uiArtifacts,
    reasoning: existing.streaming ? existing.reasoning : message.reasoning ?? existing.reasoning,
    responseProvider: message.responseProvider ?? existing.responseProvider,
    responseModel: message.responseModel ?? existing.responseModel,
  };
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
