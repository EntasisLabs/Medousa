import { describe, expect, it } from "vitest";
import { extractMarkdownHeadings } from "./headingSlug";

describe("extractMarkdownHeadings", () => {
  it("skips fenced code and collects unique slugs", () => {
    const source = [
      "# Title",
      "",
      "```",
      "## Not a heading",
      "```",
      "",
      "## Section",
      "### Detail",
      "## Section",
    ].join("\n");

    expect(extractMarkdownHeadings(source)).toEqual([
      { depth: 1, text: "Title", slug: "title" },
      { depth: 2, text: "Section", slug: "section" },
      { depth: 3, text: "Detail", slug: "detail" },
      { depth: 2, text: "Section", slug: "section-1" },
    ]);
  });
});
