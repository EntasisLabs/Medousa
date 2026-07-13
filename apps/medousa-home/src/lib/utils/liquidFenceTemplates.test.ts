import { describe, expect, it } from "vitest";
import {
  LIQUID_CHART_TEMPLATE,
  serializeChartFence,
} from "./liquidFenceTemplates";
import { preprocessLiquidEmbeds, decodeLiquidProps } from "$lib/markdown/liquidEmbeds";

describe("serializeChartFence", () => {
  it("builds a parseable bar fence", () => {
    const fence = serializeChartFence({
      type: "bar",
      title: "Test",
      seriesLabels: ["A", "B"],
      rows: [
        { category: "Jan", values: [1, 2] },
        { category: "Feb", values: [3, 4] },
      ],
    });
    const out = preprocessLiquidEmbeds(fence);
    expect(out).toContain('data-liquid-embed="chart"');
    const match = out.match(/data-liquid-props="([^"]+)"/);
    const props = decodeLiquidProps<{
      type: string;
      title?: string;
      categories: string[];
      series: { values: number[] }[];
    }>(match![1]);
    expect(props?.type).toBe("bar");
    expect(props?.title).toBe("Test");
    expect(props?.categories).toEqual(["Jan", "Feb"]);
    expect(props?.series[0].values).toEqual([1, 3]);
    expect(props?.series[1].values).toEqual([2, 4]);
  });

  it("slash chart template hydrates as chart", () => {
    const out = preprocessLiquidEmbeds(LIQUID_CHART_TEMPLATE);
    expect(out).toContain('data-liquid-embed="chart"');
  });
});
