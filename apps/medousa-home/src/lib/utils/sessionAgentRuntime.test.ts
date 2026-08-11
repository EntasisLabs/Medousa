import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearSessionAgentSessionId,
  getSessionAgentWorkId,
  setSessionAgentSessionId,
  setSessionAgentWorkId,
} from "./sessionAgentRuntime";

describe("session agent workspace metadata", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    });
  });

  afterEach(() => vi.unstubAllGlobals());

  it("distinguishes a plain chat from legacy sessions with unknown workspace metadata", () => {
    expect(getSessionAgentWorkId("chat-1")).toBeUndefined();
    setSessionAgentWorkId("chat-1", null);
    expect(getSessionAgentWorkId("chat-1")).toBeNull();
  });

  it("drops workspace metadata with the ACP session id", () => {
    setSessionAgentSessionId("chat-1", "agent-1");
    setSessionAgentWorkId("chat-1", "work-1");
    clearSessionAgentSessionId("chat-1");
    expect(getSessionAgentWorkId("chat-1")).toBeUndefined();
  });
});
