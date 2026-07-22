import { describe, expect, it } from "vitest";
import { renderInlineMarkdown, renderMarkdown } from "./render";

describe("renderInlineMarkdown", () => {
  it("renders bold and italics without block wrappers", () => {
    const html = renderInlineMarkdown(
      "2. **She Carries Two Countries in Her Voice**",
    );
    expect(html).toContain("<strong>She Carries Two Countries in Her Voice</strong>");
    expect(html).toContain("2. ");
    expect(html).not.toMatch(/<\/?p\b/);
    expect(html).not.toContain("**");
  });

  it("handles ##-style heading text with bold", () => {
    const html = renderInlineMarkdown("**Radical Emotional Honesty**");
    expect(html).toBe("<strong>Radical Emotional Honesty</strong>");
  });

  it("leaves plain text alone", () => {
    expect(renderInlineMarkdown("The Wound")).toBe("The Wound");
  });

  it("wraps tables in a scroll shell without breaking column sync", () => {
    const html = renderMarkdown(
      [
        "| Period | Fertilizer | Frequency |",
        "| --- | --- | ---: |",
        "| Spring–Summer | Balanced liquid fertilizer at half strength | Every 2 weeks |",
        "| Winter | No fertilizer | None |",
      ].join("\n"),
    );
    expect(html).toMatch(/class="[^"]*\bmarkdown-table-scroll\b[^"]*"/);
    expect(html).toMatch(/<div class="[^"]*\bmarkdown-table-scroll\b[^"]*">\s*<table>/);
    expect(html).toContain("</table></div>");
    expect(html).toContain('align="right"');
    expect(html).toContain("Every 2 weeks");
  });
});
