/**
 * Thin chat view-model reads over a session snapshot.
 * Components should use these instead of reaching into transport/transcript guts.
 */

import type { ChatMessage } from "$lib/types/chat";
import type { ChatSessionRuntime } from "$lib/chat/chatSessionRuntime";
import { loadDraftForSession } from "$lib/chat/draftPersistence";

export type ChatSelectorSnapshot = {
  sessionId: string;
  focusedSessionId: string;
  streamApplyPrincipalId: string | null;
  messages: ChatMessage[];
  draft: string;
  streamError: string | null;
  historyLoading: boolean;
  runtimes: Map<string, ChatSessionRuntime>;
};

function fieldsMatchFocused(snapshot: ChatSelectorSnapshot): boolean {
  return snapshot.streamApplyPrincipalId == null;
}

export function selectMessagesFor(
  snapshot: ChatSelectorSnapshot,
  sessionId: string,
): ChatMessage[] {
  const trimmed = sessionId.trim();
  if (!trimmed) return [];
  if (fieldsMatchFocused(snapshot) && trimmed === snapshot.sessionId) {
    return snapshot.messages;
  }
  if (
    snapshot.streamApplyPrincipalId &&
    trimmed === snapshot.streamApplyPrincipalId
  ) {
    return snapshot.runtimes.get(trimmed)?.messages ?? [];
  }
  if (trimmed === snapshot.sessionId) return snapshot.messages;
  return snapshot.runtimes.get(trimmed)?.messages ?? [];
}

export function selectDraftFor(
  snapshot: ChatSelectorSnapshot,
  sessionId: string,
): string {
  const trimmed = sessionId.trim();
  if (!trimmed) return "";
  if (fieldsMatchFocused(snapshot) && trimmed === snapshot.sessionId) {
    return snapshot.draft;
  }
  if (
    snapshot.streamApplyPrincipalId &&
    trimmed === snapshot.streamApplyPrincipalId
  ) {
    return snapshot.runtimes.get(trimmed)?.draft ?? loadDraftForSession(trimmed);
  }
  if (trimmed === snapshot.sessionId) return snapshot.draft;
  return snapshot.runtimes.get(trimmed)?.draft ?? loadDraftForSession(trimmed);
}

export function selectStreamErrorFor(
  snapshot: ChatSelectorSnapshot,
  sessionId: string,
): string | null {
  const trimmed = sessionId.trim();
  if (!trimmed) return null;
  if (fieldsMatchFocused(snapshot) && trimmed === snapshot.sessionId) {
    return snapshot.streamError;
  }
  if (
    snapshot.streamApplyPrincipalId &&
    trimmed === snapshot.streamApplyPrincipalId
  ) {
    return snapshot.runtimes.get(trimmed)?.streamError ?? null;
  }
  if (trimmed === snapshot.sessionId) return snapshot.streamError;
  return snapshot.runtimes.get(trimmed)?.streamError ?? null;
}
