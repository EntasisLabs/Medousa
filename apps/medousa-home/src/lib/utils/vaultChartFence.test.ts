import { describe, expect, it } from "vitest";
import { decodeLiquidProps, preprocessLiquidEmbeds } from "$lib/markdown/liquidEmbeds";
import {
  extractChartFences,
  parseChartFenceParts,
  replaceChartFenceAt,
  replaceChartFencePropsAt,
  summarizeChartTable,
} from "./vaultChartFence";

const SAMPLE = `Intro

\`\`\`chart
type: bar
title: Visitors
colors: blue, purple
legend: bottom

| Month | Desktop | Mobile |
| ----- | ------- | ------ |
| Jan   | 186     | 80     |
| Feb   | 305     | 200    |
\`\`\`

\`\`\`report
title: Nested

\`\`\`chart
type: radar
title: Coverage
surface: gray/12

| Axis | Alpha | Beta |
| ---- | ----- | ---- |
| Speed | 80 | 55 |
\`\`\`

More prose.
\`\`\`
`;

describe("vaultChartFence", () => {
  it("extracts charts in document order including nested", () => {
    const blocks = extractChartFences(SAMPLE);
    expect(blocks).toHaveLength(2);
    expect(blocks[0]!.body).toContain("type: bar");
    expect(blocks[1]!.body).toContain("type: radar");
  });

  it("splits KV from table without rewriting table cells", () => {
    const parts = parseChartFenceParts(extractChartFences(SAMPLE)[0]!.body);
    expect(parts.kv.type).toBe("bar");
    expect(parts.kv.title).toBe("Visitors");
    expect(parts.kv.colors).toBe("blue, purple");
    expect(parts.tableMarkdown).toContain("| Jan   | 186     | 80     |");
  });

  it("replaces KV props and preserves table markdown", () => {
    const next = replaceChartFencePropsAt(SAMPLE, 0, {
      type: "line",
      title: "Traffic",
      description: "Updated",
      legend: "top",
      labels: "category",
      surface: "",
      colors: "blue",
      seriesMarks: "",
      width: "md",
      height: "lg",
    });
    expect(next).toBeTruthy();
    expect(next!).toContain("type: line");
    expect(next!).toContain("title: Traffic");
    expect(next!).toContain("description: Updated");
    expect(next!).toContain("width: md");
    expect(next!).toContain("height: lg");
    expect(next!).toContain("| Jan   | 186     | 80     |");
    expect(next!).not.toContain("title: Visitors");

    const nested = replaceChartFencePropsAt(next!, 1, {
      type: "radar",
      title: "Team",
      description: "",
      legend: "bottom",
      labels: "",
      surface: "soft",
      colors: "purple",
      seriesMarks: "",
      width: "",
      height: "",
    });
    expect(nested!).toContain("title: Team");
    expect(nested!).toContain("surface: soft");
    expect(nested!).toContain("| Speed | 80 | 55 |");
    expect(nested!).toContain("More prose.");
  });

  it("replaces KV and table together", () => {
    const nextTable = [
      "| Category | Alpha |",
      "| --- | --- |",
      "| Q1 | 10 |",
      "| Q2 | 20 |",
    ].join("\n");
    const next = replaceChartFenceAt(
      SAMPLE,
      0,
      {
        type: "bar",
        title: "Visitors",
        description: "",
        legend: "bottom",
        labels: "",
        surface: "",
        colors: "blue, purple",
        seriesMarks: "",
        width: "",
        height: "",
      },
      nextTable,
    );
    expect(next).toBeTruthy();
    expect(next!).toContain("| Q1 | 10 |");
    expect(next!).toContain("| Q2 | 20 |");
    expect(next!).not.toContain("| Jan   | 186     | 80     |");
    expect(next!).toContain("title: Visitors");
  });

  it("summarizes chart tables for the Data fact row", () => {
    expect(summarizeChartTable("")).toBe("empty");
    expect(summarizeChartTable("not a table")).toBe("invalid table");
    const parts = parseChartFenceParts(extractChartFences(SAMPLE)[0]!.body);
    expect(summarizeChartTable(parts.tableMarkdown)).toBe("2 rows · 2 series");
  });
});

describe("preprocessLiquidEmbeds chart configure indices", () => {
  it("assigns document-order indices (nested shells live in report body)", () => {
    const src = [
      "```chart",
      "type: bar",
      "title: Outer",
      "",
      "| M | V |",
      "| - | - |",
      "| a | 1 |",
      "| b | 2 |",
      "```",
      "",
      "```report",
      "title: Nested",
      "",
      "```chart",
      "type: pie",
      "title: Inner",
      "",
      "| C | V |",
      "| - | - |",
      "| x | 3 |",
      "| y | 4 |",
      "```",
      "```",
      "",
    ].join("\n");

    const out = preprocessLiquidEmbeds(src);
    const fences = extractChartFences(src);
    expect(fences).toHaveLength(2);
    expect(out).toMatch(/liquid-chart-shell" data-edit-chart-index="0"/);

    const reportMatch = out.match(
      /data-liquid-embed="report"[^>]*data-liquid-props="([^"]+)"/,
    );
    expect(reportMatch).toBeTruthy();
    const props = decodeLiquidProps<{ body?: string }>(reportMatch![1]!);
    expect(props?.body).toContain('data-edit-chart-index="1"');
    expect(props?.body).toContain("liquid-chart-configure");
  });
});
