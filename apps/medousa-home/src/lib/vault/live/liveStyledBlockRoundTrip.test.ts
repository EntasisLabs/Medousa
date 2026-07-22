import { describe, expect, it } from "vitest";
import { liveDocToMarkdown } from "./liveDocToMarkdown";
import { markdownToLiveDoc } from "./markdownToLiveDoc";
import {
  preprocessFontSpans,
  preprocessTurboBlocks,
} from "$lib/markdown/preprocess";
import { renderMarkdown } from "$lib/markdown/render";

describe("Live styled block + block id round-trip", () => {
  it("round-trips ```block fence", () => {
    const src = [
      "```block",
      "font: serif",
      "size: lg",
      "align: center",
      "id: hero",
      "---",
      "Body line.",
      "```",
      "",
    ].join("\n");
    const doc = markdownToLiveDoc(src);
    const fence = doc.content?.find((n) => n.type === "fenceBlock");
    expect(fence?.attrs?.lang).toBe("block");
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("```block");
    expect(out).toContain("id: hero");
    expect(out).toContain("Body line.");
  });

  it("turbo-fish becomes fence atom in Live", () => {
    const src = [
      "::block::",
      "font: mono",
      "---",
      "Codey.",
      "::end::",
      "",
    ].join("\n");
    expect(preprocessTurboBlocks(src)).toContain("```block");
    const doc = markdownToLiveDoc(src);
    expect(doc.content?.some((n) => n.type === "fenceBlock")).toBe(true);
    expect(liveDocToMarkdown(doc)).toContain("```block");
  });

  it("round-trips trailing paragraph ^id", () => {
    const src = "Claim here ^claim-1\n";
    const doc = markdownToLiveDoc(src);
    const para = doc.content?.find((n) => n.type === "paragraph");
    expect(para?.attrs?.blockId).toBe("claim-1");
    const out = liveDocToMarkdown(doc);
    expect(out).toMatch(/Claim here \^claim-1/);
  });

  it("round-trips heading ^id", () => {
    const src = "## Title ^sec\n";
    const doc = markdownToLiveDoc(src);
    const heading = doc.content?.find((n) => n.type === "heading");
    expect(heading?.attrs?.blockId).toBe("sec");
    expect(liveDocToMarkdown(doc)).toMatch(/## Title \^sec/);
  });
});

describe("font marks", () => {
  it("round-trips {{font:}} and {{size:}}", () => {
    const src = "Paint {{font:serif|this}} at {{size:lg|large}}.\n";
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("{{font:serif|this}}");
    expect(out).toContain("{{size:lg|large}}");
  });

  it("preprocess emits font spans", () => {
    const out = preprocessFontSpans("{{font:mono|code}} {{size:sm|tiny}}");
    expect(out).toContain('data-md-font="mono"');
    expect(out).toContain('data-md-size="sm"');
  });
});

describe("Preview block anchors", () => {
  it("emits data-block-id on paragraphs", () => {
    const html = renderMarkdown("Hello world ^quote-of-day\n");
    expect(html).toContain('data-block-id="quote-of-day"');
    expect(html).toContain('id="^quote-of-day"');
    expect(html).not.toContain("^quote-of-day</p>");
  });
});
