/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import { loadAddressedDraft, saveAddressedDraft, savedAddress } from "./addressedDraft";
import { saveCompanionDrafts } from "./companionDrafts";
beforeEach(() => { localStorage.clear(); vi.restoreAllMocks(); });
it("restores an explicit address and draft without caching transcripts or bot profiles", () => {
  const address = savedAddress({ id: "new:codex", kind: "new", runtime: "codex", label: "Codex", group: "Start new", scope: "a", description: "" }, "created-session");
  saveAddressedDraft("a", { query: "Follow up", address });
  expect(loadAddressedDraft("a")).toEqual({ query: "Follow up", address: { kind: "new", runtime: "codex", label: "Codex", sessionId: "created-session" } });
  expect(loadAddressedDraft("b")).toEqual({ query: "", address: null });
});
it("migrates the active legacy draft once while leaving the other draft recoverable", () => {
  saveCompanionDrafts("a", { chat: "Chat draft", ask: "Task draft", mode: "ask" });
  expect(loadAddressedDraft("a")).toEqual({ query: "Task draft", address: null });
  saveAddressedDraft("a", { query: "", address: null });
  expect(loadAddressedDraft("a").query).toBe("");
  expect(localStorage.getItem("medousa-companion-drafts:a")).toContain("Chat draft");
});
it("does not turn corrupt addresses into unaddressed messages", () => {
  localStorage.setItem("medousa-companion-conversation:a", JSON.stringify({ query: "For Taco only", address: { kind: "bot", label: "Taco" } }));
  expect(loadAddressedDraft("a")).toEqual({ query: "", address: null });
});
it("storage failure does not throw through the composer", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("Full"); });
  expect(() => saveAddressedDraft("a", { query: "Keep working", address: null })).not.toThrow();
});
