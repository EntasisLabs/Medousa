import type { ExternalConversation } from "$lib/daemon/externalConversations";
import type { ChatMessage } from "$lib/types/chat";

export function externalConversationMessages(conversation: ExternalConversation | null): ChatMessage[] {
  if (!conversation) return [];
  return conversation.events.flatMap((event) => {
    const role = event.kind === "user_message" ? "user" :
      ["provider_message", "progress", "question", "completed", "failed"].includes(event.kind)
        ? "assistant" : null;
    if (!role || !event.text.trim()) return [];
    return [{
      id: `external:${conversation.id}:${event.sequence}`,
      role,
      content: event.text,
      failed: event.kind === "failed",
      lane: "chat" as const,
    }];
  });
}

export function externalConversationStatus(conversation: ExternalConversation | null): string | null {
  const kind = conversation?.events.at(-1)?.kind;
  if (kind === "transport_pending") return "Sending · outcome unknown after restart";
  if (kind === "transport_uncertain") return "Delivery unconfirmed · check the provider before resending";
  if (kind === "transport_failed") return "Send failed";
  if (kind === "transport_accepted") return conversation?.provider === "muse"
    ? "WhatsApp accepted · Muse delivery unverified"
    : "Accepted · awaiting provider reply";
  return null;
}
