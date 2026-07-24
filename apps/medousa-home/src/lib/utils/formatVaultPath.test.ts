import { describe, expect, it } from "vitest";
import { vaultPathCrumbs } from "./formatVault";

describe("vaultPathCrumbs", () => {
  it("builds folder + file crumbs", () => {
    expect(vaultPathCrumbs("inbox/ideas/slides-deck.md", "Slides Deck")).toEqual([
      { label: "Inbox", key: "inbox", kind: "folder" },
      { label: "Ideas", key: "inbox/ideas", kind: "folder" },
      { label: "Slides Deck", key: "inbox/ideas/slides-deck.md", kind: "file" },
    ]);
  });

  it("dedupes consecutive identical folder labels", () => {
    const crumbs = vaultPathCrumbs("Inbox/inbox/note.md", "Note");
    expect(crumbs.map((c) => c.label)).toEqual(["Inbox", "Note"]);
  });

  it("handles a root-level note", () => {
    expect(vaultPathCrumbs("readme.md", "Readme")).toEqual([
      { label: "Readme", key: "readme.md", kind: "file" },
    ]);
  });
});
