import { describe, expect, it } from "vitest";
import {
  buildFootnotesSectionHtml,
  collectFootnoteHits,
  extractFootnoteDefinitions,
  footnoteSlug,
  planFootnotes,
  replaceFootnoteMarkers,
} from "./footnotes";

describe("footnotes helpers", () => {
  it("extracts definitions and strips them from body", () => {
    const src = [
      "Hello[^1] world.",
      "",
      "[^1]: First note.",
      "[^two]: Second",
      "  continued line.",
      "",
    ].join("\n");
    const { body, defs } = extractFootnoteDefinitions(src);
    expect(defs.get("1")).toBe("First note.");
    expect(defs.get("two")).toBe("Second\ncontinued line.");
    expect(body).toContain("Hello[^1] world.");
    expect(body).not.toContain("[^1]:");
    expect(body).not.toContain("[^two]:");
  });

  it("skips definitions inside fenced code", () => {
    const src = ["```md", "[^1]: not a def", "```", "", "Text[^1]", "", "[^1]: real"].join(
      "\n",
    );
    const { defs, body } = extractFootnoteDefinitions(src);
    expect(defs.get("1")).toBe("real");
    expect(body).toContain("[^1]: not a def");
  });

  it("collects refs and inline in document order", () => {
    const body = "A[^b] then ^[inline] then [^a].";
    const hits = collectFootnoteHits(body);
    expect(hits.map((h) => h.id)).toEqual(["b", "inline-1", "a"]);
    expect(hits[1]?.kind).toBe("inline");
    expect(hits[1]?.text).toBe("inline");
  });

  it("skips markers inside fences when collecting", () => {
    const body = ["Before[^1]", "```", "x[^2]", "```", "After^[hi]"].join("\n");
    const hits = collectFootnoteHits(body);
    expect(hits.map((h) => h.id)).toEqual(["1", "inline-1"]);
  });

  it("numbers by first reference, then unused defs", () => {
    const src = [
      "See[^z] and [^a].",
      "",
      "[^a]: A",
      "[^z]: Z",
      "[^unused]: leftover",
    ].join("\n");
    const plan = planFootnotes(src);
    expect(plan.orderedIds).toEqual(["z", "a", "unused"]);
    expect(plan.numberById.get("z")).toBe(1);
    expect(plan.numberById.get("a")).toBe(2);
    expect(plan.numberById.get("unused")).toBe(3);
  });

  it("replaces markers with superscript HTML", () => {
    const plan = planFootnotes("Hi[^1] and ^[note].\n\n[^1]: Def.\n");
    const replaced = replaceFootnoteMarkers(plan.bodyWithoutDefs, plan.numberById);
    expect(replaced).toContain('href="#fn-1"');
    expect(replaced).toContain('href="#fn-inline-1"');
    expect(replaced).not.toContain("[^1]");
    expect(replaced).not.toContain("^[note]");
  });

  it("builds footer section", () => {
    const plan = planFootnotes("X[^1]\n\n[^1]: **bold**\n");
    const html = buildFootnotesSectionHtml(plan, (md) => `<em>${md}</em>`);
    expect(html).toContain('class="markdown-footnotes"');
    expect(html).toContain('id="fn-1"');
    expect(html).toContain("<em>**bold**</em>");
    expect(html).toContain('href="#fnref-1"');
  });

  it("slugs ids safely", () => {
    expect(footnoteSlug("My Note")).toBe("my-note");
    expect(footnoteSlug("???")).toBe("fn");
  });
});
