/** @vitest-environment happy-dom */
import { Editor } from "@tiptap/core";
import { describe, expect, it } from "vitest";
import { createLiveExtensions } from "./liveExtensions";

describe("HeadingOutlineStamp", () => {
  it("stamps live headings with outline anchors", () => {
    const editor = new Editor({
      extensions: createLiveExtensions(),
      content: "# One\n\n## Two\n\n## Two\n\n### Detail\n",
      contentType: "markdown",
    });

    const headings = [
      ...editor.view.dom.querySelectorAll<HTMLElement>("h1, h2, h3"),
    ];
    expect(headings.map((h) => h.id)).toEqual([
      "one",
      "two",
      "two-1",
      "detail",
    ]);
    for (const heading of headings) {
      expect(heading.classList.contains("markdown-heading")).toBe(true);
      expect(heading.getAttribute("data-heading-slug")).toBe(heading.id);
    }

    editor.destroy();
  });
});
