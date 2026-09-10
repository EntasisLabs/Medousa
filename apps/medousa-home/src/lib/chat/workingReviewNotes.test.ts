// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
import { readReviewNotes, reviewNotesKey, reviewNotesPrompt, writeReviewNotes, type WorkingReviewNote } from "./workingReviewNotes";
const note: WorkingReviewNote = { id:"one", path:"src/main.ts", side:"new", line:12, content:"const count = 1;", baselineOid:"abc", workingDigest:"sha256:old", body:"Handle the empty case" };
beforeEach(() => { localStorage.clear(); setActiveWorkshopIdPort(null); });
describe("working review notes", () => {
  it("preserves anchors across reloads and isolates workshops", () => {
    setActiveWorkshopIdPort(() => "workshop-a");
    const key = reviewNotesKey("work");
    writeReviewNotes(key, [note]);
    expect(readReviewNotes(key)).toEqual([note]);
    setActiveWorkshopIdPort(() => "workshop-b");
    expect(readReviewNotes(reviewNotesKey("work"))).toEqual([]);
  });
  it("does not pretend old line numbers identify current code", () => {
    const prompt = reviewNotesPrompt("HashMap", [note]);
    expect(prompt).toContain("reconcile any later edits");
    expect(prompt).toContain("src/main.ts:12");
    expect(prompt).toContain("sha256:old");
    expect(prompt).toContain(JSON.stringify(note.content));
    expect(prompt).toContain(note.body);
  });
  it("rejects malformed stored notes", () => {
    localStorage.setItem("notes", JSON.stringify([note, {body:"broken"}, {...note,line:-1}]));
    expect(readReviewNotes("notes")).toEqual([note]);
  });
});
