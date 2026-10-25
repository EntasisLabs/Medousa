import { expect, it } from "vitest";
import { referenceToken, removeReferenceToken } from "./resourceReferences";
import { composerContext } from "$lib/utils/turnContext";
import { prepareTalkAboutNote, prepareAddSelectionToChat } from "$lib/utils/vaultNoteBridge";
it("resolves a caret-scoped reference without eating the surrounding request", () => {
  const input = "Use @spec to redesign this";
  const token = referenceToken(input, 9)!;
  expect(token.query).toBe("spec");
  expect(removeReferenceToken(input, token)).toEqual({ value: "Use  to redesign this", cursor: 4 });
  expect(referenceToken("email@host.test", 15)).toBeNull();
  expect(referenceToken("Use /note design", 16)).toMatchObject({ kind: "note", query: "design" });
  expect(referenceToken("/project medousa", 16)).toMatchObject({ kind: "project", query: "medousa" });
});
it("keeps note bodies and passage snapshots out of the visible request", () => {
  const note = prepareTalkAboutNote("same.md", "Plan", "Private note body", [], []);
  expect(note.draft).toBe("Help me think through this note.");
  const selected = prepareAddSelectionToChat("same.md", "Plan", { text: "Captured passage", start: 9, end: 25 }, [], []);
  expect(selected.draft).not.toContain("Captured passage");
  const context = composerContext("notes", [], { ...selected.scope, rootId: "obsidian", workshopId: "local" }, "local")!;
  expect(context.references).toEqual([{ kind: "vault_note", root_id: "obsidian", path: "same.md", title: "Plan", selection: { text: "Captured passage" } }]);
  expect(context.document_excerpt).toBeUndefined();
});
it("does not retarget a pinned note onto a different workshop", () => {
  expect(() => composerContext("notes", [], { path: "same.md", title: "Plan", linkCount: 0, rootId: "obsidian", workshopId: "other" }, "local")).toThrow("different vault or workshop");
});
