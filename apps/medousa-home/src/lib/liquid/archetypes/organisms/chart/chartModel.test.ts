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
