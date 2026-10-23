/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it } from "vitest";
import { bindChatDraftSync, persistDraftForSession } from "./draftPersistence";

describe("cross-window composer drafts", () => {
  beforeEach(() => localStorage.clear());
  it("adopts peer edits only if the local composer still matches the peer’s base", () => {
    const host = { workshopScopeId: "one", sessionId: "chat", draft: "Before" };
    const stop = bindChatDraftSync(host);
    const change = (scope: string, before: string, after: string) => window.dispatchEvent(new StorageEvent("storage", { key: `medousa-home-chat-drafts:${scope}`, oldValue: JSON.stringify({ chat: { text: before } }), newValue: JSON.stringify({ chat: { text: after } }) }));
    change("one", "Before", "Peer edit"); expect(host.draft).toBe("Peer edit");
    host.draft = "My unsaved edit";
    change("one", "Peer edit", "Another peer edit"); expect(host.draft).toBe("My unsaved edit");
    host.draft = "Before"; change("two", "Before", "Wrong workshop"); expect(host.draft).toBe("Before");
    stop(); change("one", "Before", "After teardown"); expect(host.draft).toBe("Before");
  });
  it("does not rewrite equal drafts and cause storage-event echo loops", () => {
    persistDraftForSession("chat", "Hi", "one");
    const before = localStorage.getItem("medousa-home-chat-drafts:one");
    persistDraftForSession("chat", "Hi", "one");
    expect(localStorage.getItem("medousa-home-chat-drafts:one")).toBe(before);
  });
});
