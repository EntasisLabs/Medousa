import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { promptWithConversationContext } from "./agentConversationContext";

const message = (role: ChatMessage["role"], content: string, extra: Partial<ChatMessage> = {}): ChatMessage => ({ id: content, role, content, ...extra });
describe("conversation handoff to a fresh coding runtime", () => {
  it("carries the shared conversation in order and leaves the current request separate", () => {
    const result = promptWithConversationContext("Continue with Cursor", [message("user", "Use Svelte"), message("assistant", "The app uses Svelte 5")]);
    expect(result).toContain('[{"role":"user","content":"Use Svelte"},{"role":"assistant","content":"The app uses Svelte 5"}]');
    expect(result.endsWith("Current request:\nContinue with Cursor")).toBe(true);
  });
  it("excludes incomplete replies and unrelated background asks", () => {
    expect(promptWithConversationContext("hello", [message("assistant", "unfinished", { streaming: true }), message("user", "background", { lane: "ask" })])).toBe("hello");
  });
  it("bounds history while retaining the latest conversation", () => {
    const result = promptWithConversationContext("next", [message("user", "old".repeat(20_000)), message("assistant", "latest result")]);
    expect(result.length).toBeLessThan(49_000);
    expect(result).toContain("latest result");
  });
});
