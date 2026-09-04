import { describe, expect, it } from "vitest";
import {
  lineSimilarity,
  pairSideRows,
  tokenizeForWordDiff,
  wordDiffParts,
} from "./wordDiff";
import type { DiffLine } from "./diffTypes";

describe("tokenizeForWordDiff", () => {
  it("splits words, punctuation, and whitespace", () => {
    expect(tokenizeForWordDiff("foo(bar)")).toEqual(["foo", "(", "bar", ")"]);
  });
});

describe("wordDiffParts", () => {
  it("marks only the changed token", () => {
    const parts = wordDiffParts("return foo;", "return bar;");
    expect(parts.before.some((part) => part.changed && part.text === "foo")).toBe(true);
    expect(parts.after.some((part) => part.changed && part.text === "bar")).toBe(true);
    expect(parts.before.filter((part) => !part.changed).map((part) => part.text).join("")).toContain(
      "return",
    );
  });

  it("bounds work for generated lines instead of building a quadratic matrix", () => {
    const before = `${"old,".repeat(4_000)}tail`;
    const after = `${"new,".repeat(4_000)}tail`;
    expect(wordDiffParts(before, after)).toEqual({
      before: [{ text: before, changed: true }],
      after: [{ text: after, changed: true }],
    });
  });
});

describe("pairSideRows", () => {
  it("pairs by similarity instead of index", () => {
    const deletions: DiffLine[] = [
      { kind: "deletion", old_line: 1, content: "const alpha = 1;" },
      { kind: "deletion", old_line: 2, content: "unused()" },
    ];
    const additions: DiffLine[] = [
      { kind: "addition", new_line: 1, content: "helper()" },
      { kind: "addition", new_line: 2, content: "const alpha = 2;" },
      { kind: "addition", new_line: 3, content: "extra()" },
      { kind: "addition", new_line: 4, content: "more()" },
      { kind: "addition", new_line: 5, content: "again()" },
    ];
    const rows = pairSideRows("h", deletions, additions);
    const alpha = rows.find(
      (row) => row.oldContent.includes("alpha") || row.newContent.includes("alpha"),
    );
    expect(alpha?.kind).toBe("replacement");
    expect(alpha?.oldContent).toContain("alpha");
    expect(alpha?.newContent).toContain("alpha");
    expect(lineSimilarity("const alpha = 1;", "const alpha = 2;")).toBeGreaterThan(0.5);
  });

  it("falls back to positional pairing for very large replacement blocks", () => {
    const deletions: DiffLine[] = Array.from({ length: 65 }, (_, index) => ({
      kind: "deletion",
      old_line: index + 1,
      content: `old ${index}`,
    }));
    const additions: DiffLine[] = Array.from({ length: 65 }, (_, index) => ({
      kind: "addition",
      new_line: index + 1,
      content: `new ${index}`,
    }));
    const rows = pairSideRows("large", deletions, additions);
    expect(rows).toHaveLength(65);
    expect(rows[64]).toMatchObject({
      oldContent: "old 64",
      newContent: "new 64",
      kind: "replacement",
    });
  });
});
