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
});
