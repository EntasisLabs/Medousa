import { describe, expect, it } from "vitest";
import { findMatches } from "$lib/utils/vaultFindInNote";
import { formatVaultNoteStats, vaultNoteStats } from "$lib/utils/vaultNoteStats";

describe("vaultNoteStats", () => {
  it("counts body words without frontmatter", () => {
    const content = `---
title: Test
---

Hello world from the vault.`;
    const stats = vaultNoteStats(content);
    expect(stats.words).toBe(5);
    expect(stats.characters).toBeGreaterThan(0);
    expect(formatVaultNoteStats(stats)).toContain("words");
  });

  it("reports empty notes", () => {
    expect(formatVaultNoteStats(vaultNoteStats("---\ntitle: x\n---\n\n"))).toBe(
      "Empty note",
    );
  });
});

describe("findMatches", () => {
  it("finds case-insensitive matches", () => {
    const matches = findMatches("Hello hello HELLO", "hel");
    expect(matches).toHaveLength(3);
    expect(matches[0]).toEqual({ start: 0, end: 3 });
  });
});
