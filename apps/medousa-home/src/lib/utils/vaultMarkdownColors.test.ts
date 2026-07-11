import { describe, expect, it } from "vitest";
import { preprocessColorSpans } from "$lib/markdown/preprocess";
import {
  colorMarkupToken,
  colorSpanHtml,
  isMarkdownHexColor,
  normalizeMarkdownHexColor,
} from "./vaultMarkdownColors";
import { applyMarkdownColor } from "./vaultMarkdownEdit";

describe("hex color tokens", () => {
  it("normalizes short and long hex", () => {
    expect(normalizeMarkdownHexColor("#fff")).toBe("#FFFFFF");
    expect(normalizeMarkdownHexColor("#Ff5500")).toBe("#FF5500");
    expect(isMarkdownHexColor("#GG0000")).toBe(false);
    expect(isMarkdownHexColor("FF0000")).toBe(false);
  });

  it("serializes markup", () => {
    expect(colorMarkupToken("#f80", "sale")).toBe("{{#FF8800|sale}}");
    expect(colorMarkupToken("red", "sale")).toBe("{{red|sale}}");
  });

  it("renders safe hex spans", () => {
    expect(colorSpanHtml("#ff0000", "Alert")).toContain('style="color: #FF0000"');
    expect(colorSpanHtml("#ff0000", "Alert")).toContain("markdown-color-hex");
  });
});

describe("preprocessColorSpans hex", () => {
  it("turns {{#RGB|text}} into a colored span", () => {
    const out = preprocessColorSpans("Pay {{#F80|attention}} now");
    expect(out).toContain("markdown-color-hex");
    expect(out).toContain("#FF8800");
    expect(out).toContain("attention");
  });

  it("still supports named colors", () => {
    const out = preprocessColorSpans("{{red|Item}}");
    expect(out).toContain("markdown-color-red");
    expect(out).toContain("Item");
  });

  it("rejects invalid hex", () => {
    const src = "{{#ZZZZZZ|nope}}";
    expect(preprocessColorSpans(src)).toBe(src);
  });
});

describe("applyMarkdownColor hex", () => {
  it("wraps selection in hex markup", () => {
    const result = applyMarkdownColor("hello world", 6, 11, "#0ea5e9");
    expect(result.content).toBe("hello {{#0EA5E9|world}}");
  });
});
