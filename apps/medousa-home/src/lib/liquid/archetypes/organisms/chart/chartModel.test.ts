import { describe, expect, it } from "vitest";
import {
  chartViewModel,
  formatChartLabel,
  resolveLabelPosition,
} from "./chartModel";

describe("resolveLabelPosition", () => {
  it("returns none when labels are off", () => {
    expect(
      resolveLabelPosition({
        type: "bar",
        labels: "none",
        labelPosition: "auto",
        centerLabel: "",
        centerValue: "",
      }),
    ).toBe("none");
  });

  it("maps auto pie to outside", () => {
    expect(
      resolveLabelPosition({
        type: "pie",
        labels: "value",
        labelPosition: "auto",
        centerLabel: "",
        centerValue: "",
      }),
    ).toBe("outside");
  });

  it("maps auto donut with center chrome to outside", () => {
    expect(
      resolveLabelPosition({
        type: "donut",
        labels: "value",
        labelPosition: "auto",
        centerLabel: "Visitors",
        centerValue: "1,125",
      }),
    ).toBe("outside");
  });

  it("maps auto donut without center chrome to inside", () => {
    expect(
      resolveLabelPosition({
        type: "donut",
        labels: "value",
        labelPosition: "auto",
        centerLabel: "",
        centerValue: "",
      }),
    ).toBe("inside");
  });

  it("maps auto bar/line to outside", () => {
    expect(
      resolveLabelPosition({
        type: "bar",
        labels: "value",
        labelPosition: "auto",
        centerLabel: "",
        centerValue: "",
      }),
    ).toBe("outside");
  });

  it("respects explicit inside/outside", () => {
    expect(
      resolveLabelPosition({
        type: "pie",
        labels: "both",
        labelPosition: "inside",
        centerLabel: "",
        centerValue: "",
      }),
    ).toBe("inside");
  });
});

describe("formatChartLabel", () => {
  it("formats modes", () => {
    expect(formatChartLabel("none", "Chrome", 275)).toBe("");
    expect(formatChartLabel("category", "Chrome", 275)).toBe("Chrome");
    expect(formatChartLabel("value", "Chrome", 275)).toBe("275");
    expect(formatChartLabel("both", "Chrome", 275)).toBe("Chrome 275");
  });
});

describe("chartViewModel radar/radial", () => {
  it("accepts radar with ≥3 categories", () => {
    const model = chartViewModel({
      type: "radar",
      categories: ["A", "B", "C"],
      series: [{ key: "s", label: "Score", values: [1, 2, 3] }],
    });
    expect(model?.type).toBe("radar");
  });

  it("rejects radar with fewer than 3 categories", () => {
    expect(
      chartViewModel({
        type: "radar",
        categories: ["A", "B"],
        series: [{ key: "s", label: "Score", values: [1, 2] }],
      }),
    ).toBeNull();
  });

  it("accepts radial with a single category", () => {
    const model = chartViewModel({
      type: "radial",
      categories: ["Progress"],
      series: [{ key: "p", label: "Progress", values: [75] }],
      centerValue: "75%",
    });
    expect(model?.type).toBe("radial");
    expect(model?.centerValue).toBe("75%");
  });

  it("defaults interactive to true for hover polish", () => {
    const model = chartViewModel({
      type: "bar",
      categories: ["A", "B"],
      series: [{ key: "s", label: "S", values: [1, 2] }],
    });
    expect(model?.interactive).toBe(true);
  });

  it("honors interactive: false", () => {
    const model = chartViewModel({
      type: "pie",
      categories: ["A", "B"],
      series: [{ key: "s", label: "S", values: [1, 2] }],
      interactive: false,
    });
    expect(model?.interactive).toBe(false);
  });
});

describe("resolveChartColor", () => {
  it("maps markdown color ids to CSS vars", async () => {
    const { resolveChartColor } = await import("./chartModel");
    expect(resolveChartColor("blue")).toBe("rgb(var(--markdown-chart-blue))");
    expect(resolveChartColor("Purple")).toBe("rgb(var(--markdown-chart-purple))");
  });

  it("passes hex through", async () => {
    const { resolveChartColor } = await import("./chartModel");
    expect(resolveChartColor("#2563eb")).toBe("#2563EB");
  });

  it("uses theme chart tokens by index when override missing", async () => {
    const { chartSeriesColor } = await import("./chartModel");
    expect(chartSeriesColor(0)).toBe("rgb(var(--chart-1))");
    expect(chartSeriesColor(0, ["green", "orange"])).toBe(
      "rgb(var(--markdown-chart-green))",
    );
    expect(chartSeriesColor(1, ["green", "orange"])).toBe(
      "rgb(var(--markdown-chart-orange))",
    );
  });
});

describe("resolveChartWidth/height/surface", () => {
  it("maps width presets and lengths", async () => {
    const { resolveChartWidth } = await import("./chartModel");
    expect(resolveChartWidth("sm")).toBe("16rem");
    expect(resolveChartWidth("md")).toBe("22rem");
    expect(resolveChartWidth("lg")).toBe("28rem");
    expect(resolveChartWidth("full")).toBe("");
    expect(resolveChartWidth("70%")).toBe("70%");
    expect(resolveChartWidth("320")).toBe("320px");
  });

  it("maps height presets", async () => {
    const { resolveChartHeight } = await import("./chartModel");
    expect(resolveChartHeight("sm")).toBe("11rem");
    expect(resolveChartHeight("lg")).toBe("18rem");
    expect(resolveChartHeight("md")).toBe("");
    expect(resolveChartHeight("12rem")).toBe("12rem");
  });

  it("maps surface presets and tinted colors", async () => {
    const { resolveChartSurface } = await import("./chartModel");
    expect(resolveChartSurface("soft")).toBe("");
    expect(resolveChartSurface("none")).toBe("transparent");
    expect(resolveChartSurface("muted")).toBe("var(--chart-plot-muted)");
    expect(resolveChartSurface("blue")).toBe("rgb(var(--markdown-chart-blue) / 0.16)");
    expect(resolveChartSurface("gray")).toBe("rgb(var(--markdown-chart-gray) / 0.12)");
    expect(resolveChartSurface("gray/25")).toBe("rgb(var(--markdown-chart-gray) / 0.25)");
    expect(resolveChartSurface("grey/40")).toBe("rgb(var(--markdown-chart-gray) / 0.4)");
    expect(resolveChartSurface("soft/30")).toBe(
      "color-mix(in srgb, rgb(var(--chart-plot-ink)) 30%, transparent)",
    );
    expect(resolveChartSurface("blue/40")).toBe("rgb(var(--markdown-chart-blue) / 0.4)");
    expect(resolveChartSurface("blue @ 0.2")).toBe("rgb(var(--markdown-chart-blue) / 0.2)");
  });

  it("wires width/height/surface onto the view model", () => {
    const model = chartViewModel({
      type: "radar",
      categories: ["A", "B", "C"],
      series: [{ key: "s", label: "Score", values: [1, 2, 3] }],
      width: "sm",
      height: "lg",
      surface: "muted",
    });
    expect(model?.width).toBe("16rem");
    expect(model?.height).toBe("18rem");
    expect(model?.surface).toBe("var(--chart-plot-muted)");
  });

  it("maps surface none to transparent for plate gating", () => {
    const model = chartViewModel({
      type: "radial",
      categories: ["Desktop", "Mobile", "Tablet"],
      series: [{ key: "u", label: "Users", values: [186, 80, 120] }],
      surface: "none",
    });
    expect(model?.surface).toBe("transparent");
  });

  it("leaves surface empty when omitted so polar plates stay off", () => {
    const model = chartViewModel({
      type: "pie",
      categories: ["A", "B"],
      series: [{ key: "s", label: "S", values: [1, 2] }],
    });
    expect(model?.surface).toBe("");
  });
});

describe("chartViewModel scatter/combo/heatmap", () => {
  it("accepts scatter with ≥2 points", () => {
    const model = chartViewModel({
      type: "scatter",
      categories: ["Points"],
      series: [{ key: "points", label: "Y", values: [1, 2] }],
      points: [
        { x: 1, y: 2 },
        { x: 3, y: 4 },
      ],
    });
    expect(model?.type).toBe("scatter");
    expect(model?.points).toHaveLength(2);
  });

  it("rejects scatter with fewer than 2 points", () => {
    expect(
      chartViewModel({
        type: "scatter",
        categories: ["Points"],
        series: [{ key: "points", label: "Y", values: [1] }],
        points: [{ x: 1, y: 2 }],
      }),
    ).toBeNull();
  });

  it("defaults combo seriesMarks to bar then line", () => {
    const model = chartViewModel({
      type: "combo",
      categories: ["A", "B"],
      series: [
        { key: "r", label: "Revenue", values: [1, 2] },
        { key: "g", label: "Growth", values: [3, 4] },
      ],
    });
    expect(model?.seriesMarks).toEqual(["bar", "line"]);
  });

  it("honors explicit combo seriesMarks", () => {
    const model = chartViewModel({
      type: "combo",
      categories: ["A", "B"],
      series: [
        { key: "r", label: "Revenue", values: [1, 2] },
        { key: "g", label: "Growth", values: [3, 4] },
      ],
      seriesMarks: ["line", "bar"],
    });
    expect(model?.seriesMarks).toEqual(["line", "bar"]);
  });

  it("accepts heatmap matrix", () => {
    const model = chartViewModel({
      type: "heatmap",
      categories: ["Mon", "Tue"],
      series: [{ key: "morning", label: "Morning", values: [1, 2] }],
      matrix: {
        rows: ["Morning"],
        cols: ["Mon", "Tue"],
        values: [[1, 2]],
      },
    });
    expect(model?.type).toBe("heatmap");
    expect(model?.matrix?.values).toEqual([[1, 2]]);
  });

  it("rejects heatmap without matrix", () => {
    expect(
      chartViewModel({
        type: "heatmap",
        categories: ["Mon"],
        series: [{ key: "r", label: "R", values: [1] }],
      }),
    ).toBeNull();
  });
});
