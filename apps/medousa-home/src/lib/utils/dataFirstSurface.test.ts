import { describe, expect, it } from "vitest";
import {
  dataFirstSurfaceReady,
  ensureDataFirstSurface,
  kindFromNoteContent,
} from "./dataFirstSurface";
import { findLedgerTable } from "./markdownTable";
import { parseWorkbookManifest } from "./workbook";

describe("kindFromNoteContent", () => {
  it("prefers frontmatter kind over path", () => {
    const md = "---\nkind: sheet\n---\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    expect(kindFromNoteContent("notes/random.md", md)).toBe("sheet");
  });

  it("falls back to path when frontmatter has no kind", () => {
    expect(kindFromNoteContent("workbooks/Budget/a.md", "# hi\n")).toBe("sheet");
  });
});

describe("ensureDataFirstSurface", () => {
  it("seeds a GFM table when switching to sheet", () => {
    const next = ensureDataFirstSurface("sheet", "---\nkind: note\n---\n\n# Alone\n", "Alone");
    expect(kindFromNoteContent("x.md", next)).toBe("sheet");
    expect(findLedgerTable(next)).toBeTruthy();
    expect(dataFirstSurfaceReady("sheet", next)).toBe(true);
  });

  it("does not duplicate tables when sheet already has one", () => {
    const md =
      "---\nkind: sheet\n---\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
    const next = ensureDataFirstSurface("sheet", md, "T");
    expect(next.match(/\| --- \| --- \|/g)?.length).toBe(1);
  });

  it("seeds workbook marker when body is empty / invalid", () => {
    const next = ensureDataFirstSurface("workbook", "", "Ops");
    expect(parseWorkbookManifest(next)?.title).toBe("Ops");
    expect(parseWorkbookManifest(next)?.sheets).toContain("Sheet1");
    expect(dataFirstSurfaceReady("workbook", next)).toBe(true);
  });

  it("sets slides kind without requiring a fence", () => {
    const next = ensureDataFirstSurface("slides", "# Deck\n", "Deck");
    expect(kindFromNoteContent("decks/a.md", next)).toBe("slides");
    expect(dataFirstSurfaceReady("slides", next)).toBe(true);
  });
});
