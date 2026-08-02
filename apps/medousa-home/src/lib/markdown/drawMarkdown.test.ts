import { describe, expect, it } from "vitest";
import { createEmptyDrawDocument, serializeDrawFence } from "$lib/draw/drawDocument";
import { renderMarkdown } from "./render";

describe("drawing markdown", () => {
  it("renders draw fences as hydratable placeholders instead of code", () => {
    const html = renderMarkdown(serializeDrawFence(createEmptyDrawDocument()));
    expect(html).toContain("data-draw-embed");
    expect(html).toContain("medousa-draw-source");
    expect(html).not.toContain("markdown-code-block");
  });
});
