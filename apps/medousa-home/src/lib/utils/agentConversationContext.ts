import type { ChatMessage } from "$lib/types/chat";
import { isChatLaneMessage } from "$lib/utils/askThreads";

const MAX_HISTORY_CHARS = 48_000;

/** Seed a newly created runtime with the shared chat's recent text, once. */
export function promptWithConversationContext(prompt: string, messages: ChatMessage[]): string {
  const history: { role: string; content: string }[] = [];
  let remaining = MAX_HISTORY_CHARS;
  for (const message of [...messages].reverse()) {
    if (message.streaming || !message.content.trim() ||
      (message.role !== "user" && message.role !== "assistant") ||
      (!isChatLaneMessage(message) && message.lane !== "worker")) continue;
    const content = message.content.slice(-remaining);
    history.unshift({ role: message.role, content });
    remaining -= content.length;
    if (remaining <= 0) break;
  }
  if (history.length === 0) return prompt;
  return `The following is recent conversation history from this Medousa chat. Use it as context for the current request.\n<medousa_conversation_history>\n${JSON.stringify(history)}\n</medousa_conversation_history>\n\nCurrent request:\n${prompt}`;
}
