/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "$lib/markdown/render";

describe("guide links", () => {
  it("keeps guide: hrefs through sanitize", () => {
    const html = renderMarkdown(
      "See [Chat](guide:chat) and [Panes](guide:keyboard-flow#panes).",
    );
    expect(html).toContain('href="guide:chat"');
    expect(html).toContain('data-guide-href="guide:chat"');
    expect(html).toContain('href="guide:keyboard-flow#panes"');
  });
});
