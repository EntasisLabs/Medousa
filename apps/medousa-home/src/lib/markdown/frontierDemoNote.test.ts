import { describe, expect, it } from "vitest";
import { readFileSync } from "fs";
import { cardHasDetail, preprocessLiquidEmbeds } from "./liquidEmbeds";

function decodeLiquidProps<T>(encoded: string): T | null {
  try {
    return JSON.parse(Buffer.from(encoded, "base64").toString("utf8")) as T;
  } catch {
    return null;
  }
}

describe("frontier demo note", () => {
  it("hydrates a one-decision brief (not a component zoo)", () => {
    const src = readFileSync(
      "/Users/theelevators/medousa/frontier-models-mid-2026.md",
      "utf8",
    );
    const out = preprocessLiquidEmbeds(src);
    const kinds = new Set(
      [...out.matchAll(/data-liquid-embed="([^"]+)"/g)].map((m) => m[1]!),
    );
    for (const k of [
      "decision",
      "callout",
      "carousel",
      "compare",
      "report",
      "tabs",
      "accordion",
      "cite",
      "brief",
    ]) {
      expect(kinds.has(k), `missing ${k}`).toBe(true);
    }
    // Cut inventory organisms — one story, not a catalog.
    for (const k of [
      "dashboard",
      "chips",
      "shortlist",
      "timeline",
      "plan",
      "media",
      "steps",
      "actions",
      "code",
      "tree",
    ]) {
      expect(kinds.has(k), `should not include catalog filler ${k}`).toBe(false);
    }
    // Nested chart lives inside the report body (encoded in props).
    const reportMatch = out.match(
      /data-liquid-embed="report"[^>]*data-liquid-props="([^"]+)"/,
    );
    const report = decodeLiquidProps<{ body: string }>(reportMatch?.[1] ?? "");
    expect(report?.body).toContain('data-liquid-embed="chart"');
    expect(out).not.toContain("```report");
    expect(out).not.toContain("```chart");
  });

  it("carousel cards carry detail payloads for vault popups", () => {
    const src = readFileSync(
      "/Users/theelevators/medousa/frontier-models-mid-2026.md",
      "utf8",
    );
    const out = preprocessLiquidEmbeds(src);
    const carouselMatch = out.match(
      /data-liquid-embed="carousel"[^>]*data-liquid-props="([^"]+)"/,
    );
    const carousel = decodeLiquidProps<{
      items?: Array<{
        meta?: string;
        chips?: string[];
        points?: { label: string; body: string }[];
        summary?: string;
        body?: string;
      }>;
    }>(carouselMatch?.[1] ?? "");
    const items = carousel?.items ?? [];
    expect(items.length).toBeGreaterThanOrEqual(5);
    for (const props of items) {
      expect(
        cardHasDetail({
          meta: props.meta,
          chips: props.chips,
          points: props.points,
          summary: props.summary ?? props.body,
        }),
      ).toBe(true);
    }
  });

  it("parses nested report charts when a sibling card follows", () => {
    const src = [
      "```report",
      "title: Nested",
      "columns: 2",
      "",
      "Prose.",
      "",
      "```chart",
      "type: bar",
      "title: Visitors",
      "",
      "| Month | Desktop |",
      "| ----- | ------- |",
      "| Jan   | 186     |",
      "| Feb   | 305     |",
      "```",
      "",
      "More.",
      "```",
      "",
      "```card",
      "title: Sibling",
      "body: After the report.",
      "```",
      "",
    ].join("\n");
    const out = preprocessLiquidEmbeds(src);
    expect(out).toContain('data-liquid-embed="report"');
    expect(out).toContain('data-liquid-embed="card"');
    expect(out).not.toContain("```report");
    expect(out).not.toContain("```card");
  });
});
