import { describe, expect, it } from "vitest";
import { liveDocToMarkdown } from "./liveDocToMarkdown";
import { markdownToLiveDoc } from "./markdownToLiveDoc";
import { preprocessFootnotes } from "$lib/markdown/preprocess";

describe("Live footnote round-trip", () => {
  it("round-trips [^id] refs and definitions", () => {
    const src = ["Claim[^1] and named[^note].", "", "[^1]: One.", "[^note]: Named.", ""].join(
      "\n",
    );
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("[^1]");
    expect(out).toContain("[^note]");
    expect(out).toContain("[^1]: One.");
    expect(out).toContain("[^note]: Named.");
  });

  it("round-trips ^[inline] footnotes", () => {
    const src = "Hello ^[side note] there.\n";
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("^[side note]");
  });

  it("round-trips multi-line definition continuations", () => {
    const src = ["Text[^1]", "", "[^1]: First", "  second line", ""].join("\n");
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toMatch(/\[\^1\]:\s*First/);
    expect(out).toMatch(/ {2}second line/);
  });
});

describe("Preview footnote preprocess", () => {
  it("emits superscript refs and footer section", () => {
    const src = ["See[^1] and ^[inline].", "", "[^1]: Body text.", ""].join("\n");
    const out = preprocessFootnotes(src);
    expect(out).toContain('class="markdown-footnote-ref"');
    expect(out).toContain('href="#fn-1"');
    expect(out).toContain('id="fn-inline-1"');
    expect(out).toContain('class="markdown-footnotes"');
    expect(out).toContain("Body text.");
    expect(out).toContain("inline");
    expect(out).not.toContain("[^1]:");
  });
});
