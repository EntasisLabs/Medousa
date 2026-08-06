import { describe, expect, it } from "vitest";

import { linesInRange, splitDiffFileLines } from "./diffTypes";

describe("diff context helpers", () => {
  it("splits file text without inventing a trailing blank", () => {
    expect(splitDiffFileLines("a\nb\n")).toEqual(["a", "b"]);
    expect(splitDiffFileLines("a\nb")).toEqual(["a", "b"]);
    expect(splitDiffFileLines("")).toEqual([]);
  });

  it("returns 1-based inclusive ranges for gap expansion", () => {
    const lines = splitDiffFileLines("one\ntwo\nthree\nfour\n");
    expect(linesInRange(lines, 2, 3)).toEqual([
      { line: 2, content: "two" },
      { line: 3, content: "three" },
    ]);
    expect(linesInRange(lines, 4, 4)).toEqual([{ line: 4, content: "four" }]);
    expect(linesInRange(lines, 0, 1)).toEqual([]);
  });
});
