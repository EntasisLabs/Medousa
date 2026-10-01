import { describe, expect, it } from "vitest";
import { externalConversationBinding, externalConversationSessionId } from "./externalConversationSession";

describe("connected conversation addresses", () => {
  it("reopens the exact provider conversation without a device-local selection", () => {
    const address = externalConversationSessionId("grok_bot", "bot:prox / one");
    expect(externalConversationBinding(address)).toEqual({ provider: "grok_bot", id: "bot:prox / one" });
    expect(address).not.toBe(externalConversationSessionId("muse", "bot:prox / one"));
  });
  it("does not interpret native chats or malformed addresses as connected agents", () => {
    for (const value of ["chat-1", "external-conversation:codex:x", "external-conversation:muse:", "external-conversation:muse:%zz"]) {
      expect(externalConversationBinding(value)).toBeNull();
    }
  });
});
