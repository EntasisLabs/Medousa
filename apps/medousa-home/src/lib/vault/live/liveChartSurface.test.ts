import { describe, expect, it } from "vitest";
import {
  parseChartFenceParts,
  serializeChartFenceFromParts,
} from "$lib/utils/vaultChartFence";
import { LIQUID_CHART_ARRIVAL_TEMPLATE } from "./liveChartSurface";
import { chartFenceTemplateForType } from "$lib/utils/liquidFenceTemplates";

describe("liveChartSurface", () => {
  it("arrival template carries the liveArrival marker", () => {
    expect(LIQUID_CHART_ARRIVAL_TEMPLATE).toMatch(/liveArrival:\s*1/);
    expect(LIQUID_CHART_ARRIVAL_TEMPLATE).toContain("```chart");
  });

  it("type pick replaces arrival with a full typed template", () => {
    const next = chartFenceTemplateForType("pie");
    expect(next).toContain("type: pie");
    expect(next).not.toMatch(/liveArrival/);
    expect(next).toContain("|");
  });

  it("title/type patch preserves the table", () => {
    const raw = [
      "```chart",
      "type: bar",
      "title: Old",
      "",
      "| Month | Desktop |",
      "| --- | --- |",
      "| Jan | 100 |",
      "```",
      "",
    ].join("\n");
    const open = /^```chart[^\r\n]*\r?\n/i.exec(raw);
    const closeIdx = raw.lastIndexOf("\n```");
    const body =
      open && closeIdx > open[0].length
        ? raw.slice(open[0].length, closeIdx)
        : "";
    const parts = parseChartFenceParts(body);
    const next = serializeChartFenceFromParts(parts, {
      ...parts.kv,
      type: "area",
      title: "Trend",
    });
    expect(next).toContain("type: area");
    expect(next).toContain("title: Trend");
    expect(next).toContain("| Month | Desktop |");
  });
});
