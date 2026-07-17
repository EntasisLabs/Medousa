import { describe, expect, it } from "vitest";
import { liveDocToMarkdown } from "./liveDocToMarkdown";
import { markdownToLiveDoc } from "./markdownToLiveDoc";

describe("Live highlight + text color round-trip", () => {
  it("round-trips ==highlight==", () => {
    const src = "Hello ==world== there.\n";
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("==world==");
  });

  it("round-trips {{color|text}}", () => {
    const src = "Paint {{red|this}} please.\n";
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out).toContain("{{red|this}}");
  });

  it("round-trips hex color markup", () => {
    const src = "Hex {{#F87171|accent}}.\n";
    const doc = markdownToLiveDoc(src);
    const out = liveDocToMarkdown(doc);
    expect(out.toLowerCase()).toContain("{{#f87171|accent}}".toLowerCase());
  });
});
