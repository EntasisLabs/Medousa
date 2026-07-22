import { describe, expect, it } from "vitest";
import { preprocessCallouts } from "./preprocess";

describe("preprocessCallouts", () => {
  it("renders icon + title header with span title (no paragraph margin leak)", () => {
    const html = preprocessCallouts("> [!note] Heads up\n> Body line\n");
    expect(html).toContain('class="markdown-callout-header"');
    expect(html).toContain('class="markdown-callout-icon"');
    expect(html).toContain('<span class="markdown-callout-title">Heads up</span>');
    expect(html).not.toMatch(/<p class="markdown-callout-title"/);
    expect(html).toContain('data-callout="note"');
    expect(html).toContain("<p>Body line</p>");
  });

  it("falls back for unknown tone and still renders a header", () => {
    const html = preprocessCallouts("> [!weirdtone] Custom\n");
    expect(html).toContain('data-callout="weirdtone"');
    expect(html).toContain('class="markdown-callout-icon"');
    expect(html).toContain('<span class="markdown-callout-title">Custom</span>');
  });
});
