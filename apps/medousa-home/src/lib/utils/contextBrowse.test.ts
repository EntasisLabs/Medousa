import { expect, it } from "vitest";
import { contextBrowseEntries, contextMemoryPresentation, relatedContextNotes, contextNotesForSession } from "./contextBrowse";
import { sessionDisplayName } from "./contextHuman";
const moments = [{ sync_key: "record-1", session_id: "chat-1", tier: "raw", timestamp: "2026-10-16T12:00:00Z", context_summary: "Checkpoint: folder paging finished", psi: 0, rho: 0, kappa: 0 }];
it("browses intact saved memory and searches source sessions", () => {
  const entries = contextBrowseEntries(moments, { "chat-1": "Files implementation" });
  expect(entries.map((entry) => entry.id)).toEqual(["thread:record-1"]);
  expect(entries[0].title).toBe("folder paging finished");
  expect(contextMemoryPresentation(moments[0]).original).toBe(moments[0].context_summary);
  expect(contextBrowseEntries(moments, { "chat-1": "Files implementation" }, "implementation")).toHaveLength(1);
  expect(contextBrowseEntries(moments, {}, "unrelated document")).toHaveLength(0);
});
it("includes only explicitly session-linked notes and resolves the longest session prefix", () => {
  const notes = [
    { path: "linked.md", title: "Linked", modified_at_utc: "", tags: ["chat:session-a-long"] },
    { path: "other.md", title: "Other", modified_at_utc: "", tags: ["shared-topic"] },
    { path: "unknown.md", title: "Unknown", modified_at_utc: "", tags: ["chat:unloaded"] },
  ];
  const sessions = ["session-a", "session-a-longer"];
  expect(relatedContextNotes(notes, sessions).map((note) => note.path)).toEqual(["linked.md"]);
  expect(contextNotesForSession(notes, "session-a", sessions)).toEqual([]);
  expect(contextNotesForSession(notes, "session-a-longer", sessions).map((note) => note.path)).toEqual(["linked.md"]);
});
it("labels delegated source sessions by project without exposing their internal ID as the title", () => {
  expect(sessionDisplayName("coder:/Users/example/Medousa/.git:work-123:abc:g1")).toBe("Medousa · delegated session");
  expect(sessionDisplayName("chat-a", { "chat-a": "Planning" })).toBe("Planning");
});

it("cleans display wrappers while keeping full memory text and failure meaning", () => {
  const node = { tier: "raw", context_summary: "verification: Verification completed: Check worktree and test baseline" };
  expect(contextMemoryPresentation(node)).toMatchObject({ kind: "Verification", title: "Check worktree and test baseline", original: node.context_summary });
  expect(contextMemoryPresentation({ ...node, context_summary: "open_gap: Verification failed: Run the matrix" })).toMatchObject({ kind: "Verification failed", title: "Run the matrix" });
  const long = "handoff: Handoff: " + "A long saved sentence with important context. ".repeat(15);
  const presentation = contextMemoryPresentation({ ...node, context_summary: long });
  expect(presentation.title.length).toBeLessThanOrEqual(140);
  expect(presentation.content).toContain("A long saved sentence");
  expect(presentation.original).toBe(long);
  expect(contextBrowseEntries([{ ...moments[0], context_summary: long + "find-this-at-the-end" }], {}, "find-this-at-the-end")).toHaveLength(1);
});
