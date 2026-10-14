/** @vitest-environment happy-dom */
import { beforeEach, expect, it } from "vitest";
import { loadCompanionDrafts, saveCompanionDrafts } from "./companionDrafts";

beforeEach(() => localStorage.clear());

it("restores both drafts and the selected mode after a companion is recreated", () => {
  const drafts = { chat: "unfinished chat", ask: "unfinished task", mode: "ask" as const };
  saveCompanionDrafts("local", drafts);
  expect(loadCompanionDrafts("local")).toEqual(drafts);
  saveCompanionDrafts("local", { ...drafts, ask: "" });
  expect(loadCompanionDrafts("local").ask).toBe("");
});

it("keeps companion drafts scoped to the workshop", () => {
  saveCompanionDrafts("local", { chat: "local draft", ask: "", mode: "chat" });
  saveCompanionDrafts("remote", { chat: "remote draft", ask: "task", mode: "ask" });
  expect(loadCompanionDrafts("local").chat).toBe("local draft");
  expect(loadCompanionDrafts("remote").chat).toBe("remote draft");
});

it("opens with an empty composer when persisted state is corrupt", () => {
  localStorage.setItem("medousa-companion-drafts:local", "not-json");
  expect(loadCompanionDrafts("local")).toEqual({ chat: "", ask: "", mode: "chat" });
});
