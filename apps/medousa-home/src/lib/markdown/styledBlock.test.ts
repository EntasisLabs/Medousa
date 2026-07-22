import { describe, expect, it } from "vitest";
import {
  parseStyledBlockBody,
  serializeStyledBlockFence,
} from "./styledBlock";
import { preprocessTurboBlocks, preprocessWikilinks } from "./preprocess";
import {
  extractTrailingBlockId,
  normalizeBlockId,
} from "./blockAnchors";

describe("styled block fence", () => {
  it("parses and serializes meta + body", () => {
    const src = [
      "id: quote",
      "font: serif",
      "size: lg",
      "align: center",
      "spacing: relaxed",
      "---",
      "Centered line.",
    ].join("\n");
    const parsed = parseStyledBlockBody(src);
    expect(parsed).toMatchObject({
      id: "quote",
      font: "serif",
      size: "lg",
      align: "center",
      spacing: "relaxed",
      body: "Centered line.",
    });
    const fence = serializeStyledBlockFence(parsed!);
    expect(fence).toContain("```block");
    expect(fence).toContain("id: quote");
    expect(fence).toContain("Centered line.");
  });
});

describe("preprocessTurboBlocks", () => {
  it("rewrites ::block:: … ::end:: to ```block fence", () => {
    const src = [
      "::block::",
      "font: serif",
      "align: center",
      "id: hero-line",
      "---",
      "Centered serif body.",
      "::end::",
      "",
    ].join("\n");
    const out = preprocessTurboBlocks(src);
    expect(out).toContain("```block");
    expect(out).toContain("font: serif");
    expect(out).toContain("id: hero-line");
    expect(out).toContain("Centered serif body.");
    expect(out).not.toContain("::block::");
  });

  it("skips turbo-fish inside fences", () => {
    const src = ["```md", "::block::", "---", "x", "::end::", "```", ""].join(
      "\n",
    );
    expect(preprocessTurboBlocks(src)).toContain("::block::");
  });
});

describe("block anchors", () => {
  it("extracts trailing ^id", () => {
    expect(extractTrailingBlockId("Hello world ^quote")).toEqual({
      text: "Hello world",
      blockId: "quote",
    });
    expect(normalizeBlockId("^quote")).toBe("quote");
  });

  it("does not treat footnotes as block ids", () => {
    expect(extractTrailingBlockId("See[^1]")).toEqual({
      text: "See[^1]",
      blockId: null,
    });
  });

  it("preprocesses [[#^id]] same-note wikilinks", () => {
    const out = preprocessWikilinks("Jump [[#^quote]]");
    expect(out).toContain("wikilink:");
    expect(decodeURIComponent(out)).toContain("#^quote");
  });
});
