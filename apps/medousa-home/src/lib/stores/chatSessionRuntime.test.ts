import { describe, expect, it } from "vitest";
import { cloneRuntime, emptySessionRuntime } from "./chatSessionRuntime";

describe("chatSessionRuntime", () => {
  it("creates an empty runtime", () => {
    const runtime = emptySessionRuntime("sess-a", "hello");
    expect(runtime.sessionId).toBe("sess-a");
    expect(runtime.draft).toBe("hello");
    expect(runtime.messages).toEqual([]);
    expect(runtime.turns.size).toBe(0);
  });

  it("clones without sharing maps", () => {
    const runtime = emptySessionRuntime("sess-a");
    runtime.messages.push({
      id: "m1",
      role: "user",
      content: "hi",
      createdAt: new Date().toISOString(),
    } as never);
    runtime.turns.set("t1", {
      turnId: "t1",
      mode: "interactive",
      phase: "running",
      messageId: null,
      streamAttached: true,
      terminal: false,
      workspaceCardId: null,
    } as never);
    const copy = cloneRuntime(runtime);
    copy.messages.push({
      id: "m2",
      role: "assistant",
      content: "yo",
      createdAt: new Date().toISOString(),
    } as never);
    expect(runtime.messages).toHaveLength(1);
    expect(copy.messages).toHaveLength(2);
    expect(copy.turns).not.toBe(runtime.turns);
  });
});
