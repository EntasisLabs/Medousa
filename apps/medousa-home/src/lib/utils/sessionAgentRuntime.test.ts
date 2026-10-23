import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearSessionAgentSessionId,
  getSessionAgentWorkId,
  getSessionAgentRuntime,
  getSessionAgentSessionId,
  isExternalAgentRuntime,
  isProviderConversationRuntime,
  setSessionAgentRuntime,
  setSessionAgentSessionId,
  setSessionAgentWorkId,
  agentConversationContextSeeded,
  markAgentConversationContextSeeded,
} from "./sessionAgentRuntime";
import { setActiveWorkshopIdPort } from "./workshopLocality";
import { EXTERNAL_AGENTS } from "./externalAgentCatalog";
import { externalConversationSessionId } from "./externalConversationSession";

describe("session agent workspace metadata", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    });
  });

  afterEach(() => { vi.unstubAllGlobals(); setActiveWorkshopIdPort(null); });

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

  it("keeps provider conversations distinct from ACP agent sessions", () => {
    setSessionAgentRuntime("chat-1", "muse");
    expect(getSessionAgentRuntime("chat-1")).toBe("muse");
    expect(isProviderConversationRuntime(getSessionAgentRuntime("chat-1"))).toBe(true);
    expect(isExternalAgentRuntime(getSessionAgentRuntime("chat-1"))).toBe(false);
    setSessionAgentRuntime("chat-1", "grok_bot");
    expect(getSessionAgentRuntime("chat-1")).toBe("muse");
  });

  it("locks a connected conversation on reopen even with a stale runtime preference", () => {
    const sessionId = externalConversationSessionId("dots", "dot-a");
    setSessionAgentRuntime(sessionId, "codex");
    expect(getSessionAgentRuntime(sessionId)).toBe("dots");
  });

  it("seeds each new runtime process once while keeping ordinary chats swappable", () => {
    setSessionAgentRuntime("chat-1", "codex");
    markAgentConversationContextSeeded("chat-1", "agent-codex");
    expect(agentConversationContextSeeded("chat-1", "agent-codex")).toBe(true);
    setSessionAgentRuntime("chat-1", "cursor");
    expect(getSessionAgentRuntime("chat-1")).toBe("cursor");
    expect(agentConversationContextSeeded("chat-1", "agent-cursor")).toBe(false);
  });
  it.each(EXTERNAL_AGENTS)("restores persisted $id sessions without changing their identity", agent => {
    setSessionAgentRuntime("new-agent", agent.id);
    setSessionAgentSessionId("new-agent", "provider-session");
    expect(getSessionAgentRuntime("new-agent")).toBe(agent.id);
    expect(isExternalAgentRuntime(getSessionAgentRuntime("new-agent"))).toBe(true);
  });

  it.each(EXTERNAL_AGENTS)("keeps $id preferences and provider sessions isolated between workshops", agent => {
    let scope = "studio";
    setActiveWorkshopIdPort(() => scope);
    setSessionAgentRuntime("same-session", agent.id);
    setSessionAgentSessionId("same-session", "provider-studio");
    scope = "remote";
    expect(getSessionAgentRuntime("same-session")).toBe("medousa");
    expect(getSessionAgentSessionId("same-session")).toBeNull();
    scope = "studio";
    expect(getSessionAgentRuntime("same-session")).toBe(agent.id);
    expect(getSessionAgentSessionId("same-session")).toBe("provider-studio");
  });

});
