import { describe, expect, it } from "vitest";
import {
  normalizeLiquidMediaSource,
  prepareObsidianLiquidMarkdown,
} from "./liquidMarkdown";

describe("Obsidian Liquid Markdown adapter", () => {
  it("prepares shared placeholders and removes Home-only chart controls", () => {
    const card = prepareObsidianLiquidMarkdown("```card\ntitle: Vault native\n```");
    expect(card).toContain('data-liquid-embed="card"');

    const chart = prepareObsidianLiquidMarkdown([
      "```chart",
      "type: bar",
      "| Month | Visits |",
      "| ----- | ------ |",
      "| Jan | 12 |",
      "| Feb | 18 |",
      "```",
    ].join("\n"));
    expect(chart).toContain('data-liquid-embed="chart"');
    expect(chart).not.toContain("liquid-chart-configure");
  });

  it("normalizes vault media paths without rewriting URLs", () => {
    expect(normalizeLiquidMediaSource("<assets/cover art.png>")).toBe("assets/cover art.png");
    expect(normalizeLiquidMediaSource("https://example.test/image.png")).toBe("https://example.test/image.png");
    expect(normalizeLiquidMediaSource("app://local/image.png")).toBe("app://local/image.png");
  });
});
