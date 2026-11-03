/**
 * Presentation helpers for chat turns — hide empty worker/workshop handoff
 * shells so the UI shows one substance surface (synthesis / body), not an ack bubble.
 *
 * Painted-id authority for placement lives in Rust
 * (`medousa_app_core::display::present_chat_items` / `ProjectVisibleItems`).
 * This module stays as the sync paint path (and worker-panel collapse); keep
 * `presentChatMessages` aligned with the Rust filter.
 */

import type { ChatMessage } from "$lib/types/chat";

/** Completed handoff ack with no substance — do not paint as its own bubble. */
export function isEmptyHandoffShell(message: ChatMessage): boolean {
  if (message.role !== "assistant") return false;
  if (message.streaming) return false;
  if (message.failed) return false;
  if (message.content?.trim()) return false;
  if (message.toolRuns && message.toolRuns.length > 0) return false;
  if (message.tools && message.tools.length > 0) return false;
  if (message.uiArtifacts && message.uiArtifacts.length > 0) return false;
  if (message.mediaAttachments?.length) return false;
  if (message.reasoning?.trim()) return false;
  if (message.budgetRequestId) return false;
  // Handoff shells carry a stage whisper (or were cleared to whisper-only).
  return Boolean(message.stageWhisper?.trim());
}

/**
 * Messages to paint in the main chat list: drop empty handoff shells.
 * Store may still keep them for worker linkage / history.
 */
export function presentChatMessages(
  messages: ChatMessage[],
  threads: ReadonlyMap<string, { returnedTexts?: string[] }> = new Map(),
): ChatMessage[] {
  return messages.filter((message) => {
    if (isEmptyHandoffShell(message)) return false;
    // Only suppress a linked raw return, never every worker-lane message:
    // that lane also carries host-authored synthesis and failure/retry UI.
    if (message.role !== 'assistant' || message.lane !== 'worker' || !message.workId
      || message.failed || message.budgetRequestId) return true;
    const thread = threads.get(message.workId);
    if (!thread) return true;
    const text = message.content.trim();
    return !text || !thread.returnedTexts?.some(result => result.trim() === text);
  });
}

/**
 * Collapse a worker thread to a single visual turn when possible.
 * Prefers the synthesis (content / streaming) message; folds handoff whisper
 * into status when the synthesis row would otherwise lack a pulse label.
 */
export function presentWorkerThreadMessages(messages: ChatMessage[]): ChatMessage[] {
  if (messages.length <= 1) return presentChatMessages(messages);

  const substance = [...messages].reverse().find(
    (m) =>
      m.role === "assistant" &&
      (Boolean(m.content?.trim()) ||
        Boolean(m.streaming) ||
        Boolean(m.failed) ||
        Boolean(m.toolRuns?.length) ||
        Boolean(m.uiArtifacts?.length) ||
        Boolean(m.mediaAttachments?.length)),
  );

  if (!substance) return presentChatMessages(messages);

  const handoff = messages.find(
    (m) => m.id !== substance.id && isEmptyHandoffShell(m),
  );
  if (!handoff?.stageWhisper?.trim()) {
    return [substance];
  }

  if (substance.statusLine?.trim() || substance.content?.trim()) {
    return [substance];
  }

  return [
    {
      ...substance,
      statusLine: substance.statusLine ?? handoff.stageWhisper,
      stageWhisper: substance.stageWhisper ?? handoff.stageWhisper,
    },
  ];
}
