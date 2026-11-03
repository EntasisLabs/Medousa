import type { ChatMessage } from "$lib/types/chat";

/**
 * Keep first occurrence — Svelte keyed `{#each}` throws on duplicate message ids.
 *
 * Live↔persisted reconciliation lives in `medousa-app-core` (Rust). Do not
 * reintroduce a TypeScript merge engine here.
 */
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
