import { describe, expect, it } from "vitest";
import { highlightDiffLine, languageHintForPath } from "./highlightDiffLine";

describe("languageHintForPath", () => {
  it("resolves common extensions", () => {
    expect(languageHintForPath("apps/home/src/foo.ts")).toBe("typescript");
    expect(languageHintForPath("src/main.rs")).toBe("rust");
    expect(languageHintForPath("README")).toBe("plaintext");
  });
});

describe("highlightDiffLine", () => {
  it("returns plain spans for plaintext", () => {
    const spans = highlightDiffLine("hello world", "plaintext");
    expect(spans.map((span) => span.text).join("")).toBe("hello world");
  });

  it("memoizes identical inputs", () => {
    const first = highlightDiffLine("const x = 1;", "typescript");
    const second = highlightDiffLine("const x = 1;", "typescript");
    expect(second).toBe(first);
  });

  it("leaves pathological generated lines plain", () => {
    const line = `const generated = "${"x".repeat(12_100)}";`;
    expect(highlightDiffLine(line, "typescript")).toEqual([
      { text: line, style: null },
    ]);
  });
});
