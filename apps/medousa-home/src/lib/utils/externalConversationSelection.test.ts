import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getExternalConversationSelection, setExternalConversationSelection } from "./externalConversationSelection";

describe("external conversation selection", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("remembers each provider choice per chat", () => {
    setExternalConversationSelection("chat-1", "muse", "muse-a");
    setExternalConversationSelection("chat-1", "grok_bot", "grok-a");
    setExternalConversationSelection("chat-2", "muse", "muse-b");
    expect(getExternalConversationSelection("chat-1", "muse")).toBe("muse-a");
    expect(getExternalConversationSelection("chat-1", "grok_bot")).toBe("grok-a");
    expect(getExternalConversationSelection("chat-2", "muse")).toBe("muse-b");
  });
});
