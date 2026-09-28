import { describe, expect, it } from "vitest";
import { externalConversationMessages, externalConversationStatus } from "./externalConversationMessages";
import type { ExternalConversation } from "$lib/daemon/externalConversations";

describe("external conversation projection", () => {
  it("shows user and provider messages without transport bookkeeping", () => {
    const conversation = {
      id: "session-1",
      events: [
        { sequence: 1, kind: "user_message", text: "Hello" },
        { sequence: 2, kind: "transport_accepted", text: "Transport accepted the message" },
        { sequence: 3, kind: "provider_message", text: "Hi back" },
      ],
    } as ExternalConversation;
    expect(externalConversationMessages(conversation).map((message) => [message.role, message.content]))
      .toEqual([["user", "Hello"], ["assistant", "Hi back"]]);
    expect(externalConversationStatus(conversation)).toBeNull();
    conversation.events.pop();
    expect(externalConversationStatus(conversation)).toBe("Accepted · awaiting provider reply");
    conversation.provider = "muse";
    expect(externalConversationStatus(conversation)).toBe("WhatsApp accepted · Muse delivery unverified");
  });
});
