import { describe, expect, it } from "vitest";
import {
  activeMarkdownFormats,
  backspaceListPrefix,
  continueListOnEnter,
  findHeadingSourceOffset,
} from "./vaultMarkdownEdit";
import {
  findMatches,
  replaceAllFindMatches,
  replaceFindMatch,
} from "./vaultFindInNote";

describe("continueListOnEnter", () => {
  it("continues a bullet list", () => {
    const content = "- item";
    const result = continueListOnEnter(content, content.length);
    expect(result?.content).toBe("- item\n- ");
    expect(result?.selectionStart).toBe("- item\n- ".length);
  });

  it("continues a numbered list with incremented index", () => {
    const content = "1. first";
    const result = continueListOnEnter(content, content.length);
    expect(result?.content).toBe("1. first\n2. ");
  });

  it("continues a checkbox as unchecked", () => {
    const content = "- [x] done";
    const result = continueListOnEnter(content, content.length);
    expect(result?.content).toBe("- [x] done\n- [ ] ");
  });

  it("exits an empty list line", () => {
    const content = "- ";
    const result = continueListOnEnter(content, content.length);
    expect(result?.content).toBe("");
    expect(result?.selectionStart).toBe(0);
  });

  it("returns null off a list line", () => {
    expect(continueListOnEnter("plain", 5)).toBeNull();
  });
});

describe("backspaceListPrefix", () => {
  it("removes a bullet marker", () => {
    const content = "- hello";
    const result = backspaceListPrefix(content, 2);
    expect(result?.content).toBe("hello");
    expect(result?.selectionStart).toBe(0);
  });

  it("keeps indent when removing a marker", () => {
    const content = "  - hello";
    const result = backspaceListPrefix(content, 4);
    expect(result?.content).toBe("  hello");
    expect(result?.selectionStart).toBe(2);
  });

  it("returns null when not after a marker", () => {
    expect(backspaceListPrefix("- hello", 5)).toBeNull();
  });
});

describe("activeMarkdownFormats", () => {
  it("detects bold and highlight wrappers", () => {
    expect(activeMarkdownFormats("**x**", 0, 5)).toContain("bold");
    expect(activeMarkdownFormats("==x==", 0, 5)).toContain("highlight");
  });
});

describe("findHeadingSourceOffset", () => {
  it("finds a matching heading line", () => {
    const content = "intro\n\n## Hello World\n\nbody";
    expect(findHeadingSourceOffset(content, "Hello World")).toBe(7);
  });
});

describe("findMatches case sensitivity", () => {
  it("matches case-insensitively by default", () => {
    expect(findMatches("Hello hello", "HELLO")).toEqual([
      { start: 0, end: 5 },
      { start: 6, end: 11 },
    ]);
  });

  it("respects match-case", () => {
    expect(findMatches("Hello hello", "Hello", { caseSensitive: true })).toEqual([
      { start: 0, end: 5 },
    ]);
  });
});

describe("replace find helpers", () => {
  it("replaces one match", () => {
    const result = replaceFindMatch("aaa", { start: 1, end: 2 }, "X");
    expect(result.content).toBe("aXa");
  });

  it("replaces all matches", () => {
    const result = replaceAllFindMatches("a a a", "a", "b");
    expect(result.content).toBe("b b b");
    expect(result.count).toBe(3);
  });
});
