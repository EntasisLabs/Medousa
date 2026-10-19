import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./render";

describe("Markdown link contents", () => {
  it("renders linked image badges inside their anchor without leaking Markdown syntax", () => {
    const html = renderMarkdown('[![Compatibility score](https://example.com/badge.svg)](https://example.com/details)');
    expect(html).toContain('<a href="https://example.com/details"');
    expect(html).toContain('<span class="markdown-image markdown-image-inline"><img src="https://example.com/badge.svg" alt="Compatibility score"');
    expect(html).not.toContain('![Compatibility score]');
    expect(html).not.toContain('<figure');
  });
  it("preserves inline formatting inside links while standalone images keep their figure", () => {
    expect(renderMarkdown('[**Release** `2.0.8`](https://example.com/releases)')).toContain('<strong>Release</strong> <code>2.0.8</code></a>');
    expect(renderMarkdown('![Diagram](https://example.com/diagram.png)')).toContain('<figure class="markdown-image">');
  });
});
