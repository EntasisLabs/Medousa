import { describe, expect, it } from "vitest";
import { preprocessLiquidEmbeds, decodeLiquidProps } from "$lib/markdown/liquidEmbeds";
import { LIQUID_SLIDES_TEMPLATE } from "./liquidFenceTemplates";
import {
  noteHasSlidesDeck,
  parseSlidesDeck,
  serializeSlidesDeckBody,
  serializeSlidesFence,
  splitTopLevelSectionBreaks,
} from "./markdownSlides";
import { prepareSlidesExportMarkdown } from "./vaultExportPrep";

describe("markdownSlides", () => {
  it("parses nest-aware --- sections with a chart fence inside", () => {
    const body = [
      "title: Pitch",
      "theme: paper",
      "columns: 2",
      "",
      "---",
      "label: Title",
      "layout: hero",
      "",
      "# Hello",
      "",
      "---",
      "label: Price",
      "layout: split",
      "",
      "Prose beside chart",
      "",
      "```chart",
      "type: line",
      "title: Price",
      "",
      "| Month | Price |",
      "| ----- | ----- |",
      "| Jan   | 12    |",
      "```",
      "",
    ].join("\n");

    const deck = parseSlidesDeck(body);
    expect(deck).not.toBeNull();
    expect(deck!.title).toBe("Pitch");
    expect(deck!.slides).toHaveLength(2);
    expect(deck!.slides[0]!.layout).toBe("hero");
    expect(deck!.slides[1]!.body).toContain("```chart");
    expect(deck!.slides[1]!.body).toContain("type: line");

    const parts = splitTopLevelSectionBreaks(body);
    // preamble + 2 sections
    expect(parts.length).toBe(3);
  });

  it("round-trips serialize → parse", () => {
    const deck = parseSlidesDeck(
      [
        "title: Deck",
        "columns: 3",
        "",
        "---",
        "label: A",
        "layout: stack",
        "",
        "Body A",
        "",
        "---",
        "label: B",
        "layout: split",
        "",
        "Body B",
      ].join("\n"),
    )!;
    const again = parseSlidesDeck(serializeSlidesDeckBody(deck));
    expect(again?.title).toBe("Deck");
    expect(again?.columns).toBe("3");
    expect(again?.slides.map((s) => s.label)).toEqual(["A", "B"]);
    expect(serializeSlidesFence(deck)).toContain("```slides");
  });

  it("detects kind: slides / medousa-deck", () => {
    expect(noteHasSlidesDeck("---\nkind: slides\n---\n\n# Hi")).toBe(true);
    expect(
      noteHasSlidesDeck("---\nkind: note\nmedousa-deck: basic\n---\n\nx"),
    ).toBe(true);
    expect(noteHasSlidesDeck("---\nkind: note\n---\n\n# Hi")).toBe(false);
  });

  it("preprocesses slides fence with nested chart", () => {
    const out = preprocessLiquidEmbeds(LIQUID_SLIDES_TEMPLATE);
    expect(out).toContain('data-liquid-embed="slides"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    expect(match).toBeTruthy();
    const props = decodeLiquidProps<{
      title?: string;
      slides: { label: string; body: string }[];
    }>(match![1]);
    expect(props?.title).toBe("Mid-2026 pitch");
    expect(props?.slides?.length).toBeGreaterThanOrEqual(2);
    const price = props?.slides.find((s) => s.label === "Price story");
    expect(price?.body).toMatch(/Prose wraps beside the chart/);
    // Innermost-first preprocess turns nested ```chart into a placeholder host.
    expect(price?.body).toMatch(/data-liquid-embed="chart"|```chart/);
  });

  it("wraps whole-note decks for export prep", () => {
    const note = [
      "---",
      "kind: slides",
      "medousa-deck: basic",
      "---",
      "",
      "title: Pitch",
      "columns: 2",
      "",
      "---",
      "label: Title",
      "layout: hero",
      "",
      "# Hello",
    ].join("\n");
    const wrapped = prepareSlidesExportMarkdown(note);
    expect(wrapped).toContain("```slides");
    expect(wrapped).toContain("kind: slides");
    expect(prepareSlidesExportMarkdown(wrapped)).toContain("```slides");
  });
});
