/**
 * Paste-ready Liquid fence templates for vault slash insert and agent-facing docs.
 * Contract matches preprocessLiquidEmbeds / parseChartBody in liquidEmbeds.ts.
 */

export type ChartFenceType =
  | "bar"
  | "line"
  | "area"
  | "pie"
  | "donut"
  | "radar"
  | "radial";

export interface ChartFenceRow {
  category: string;
  values: number[];
}

export interface SerializeChartFenceOptions {
  type: ChartFenceType;
  title?: string;
  description?: string;
  seriesLabels: string[];
  rows: ChartFenceRow[];
  extraKv?: Record<string, string>;
}

function escapeCell(value: string): string {
  return value.replace(/\|/g, "\\|").trim() || "—";
}

/** Build a ```chart fence from KV + tabular rows. */
export function serializeChartFence(options: SerializeChartFenceOptions): string {
  const { type, title, description, seriesLabels, rows, extraKv } = options;
  if (seriesLabels.length < 1 || rows.length < 1) {
    throw new Error("serializeChartFence requires at least one series and one row");
  }

  const lines: string[] = ["```chart", `type: ${type}`];
  if (title?.trim()) lines.push(`title: ${title.trim()}`);
  if (description?.trim()) lines.push(`description: ${description.trim()}`);
  if (extraKv) {
    for (const [key, value] of Object.entries(extraKv)) {
      if (value.trim()) lines.push(`${key}: ${value.trim()}`);
    }
  }
  lines.push("");

  const header = ["Category", ...seriesLabels.map(escapeCell)];
  lines.push(`| ${header.join(" | ")} |`);
  lines.push(`| ${header.map(() => "---").join(" | ")} |`);
  for (const row of rows) {
    const cells = [
      escapeCell(row.category),
      ...seriesLabels.map((_, i) => String(row.values[i] ?? 0)),
    ];
    lines.push(`| ${cells.join(" | ")} |`);
  }
  lines.push("```", "");
  return lines.join("\n");
}

export const LIQUID_CHART_BAR_TEMPLATE = serializeChartFence({
  type: "bar",
  title: "Visitors",
  description: "Last months",
  seriesLabels: ["Desktop", "Mobile"],
  rows: [
    { category: "Jan", values: [186, 80] },
    { category: "Feb", values: [305, 200] },
    { category: "Mar", values: [237, 120] },
  ],
  extraKv: { legend: "bottom", colors: "blue, purple" },
});

export const LIQUID_CHART_LINE_TEMPLATE = serializeChartFence({
  type: "line",
  title: "Trend",
  seriesLabels: ["Desktop", "Mobile"],
  rows: [
    { category: "Jan", values: [186, 80] },
    { category: "Feb", values: [305, 200] },
    { category: "Mar", values: [237, 120] },
  ],
  extraKv: { curve: "smooth", legend: "bottom" },
});

export const LIQUID_CHART_PIE_TEMPLATE = serializeChartFence({
  type: "pie",
  title: "Share",
  seriesLabels: ["Share"],
  rows: [
    { category: "Chrome", values: [275] },
    { category: "Safari", values: [200] },
    { category: "Firefox", values: [187] },
  ],
  extraKv: { labels: "both", labelPosition: "outside", legend: "bottom" },
});

export const LIQUID_CHART_RADAR_TEMPLATE = serializeChartFence({
  type: "radar",
  title: "Coverage",
  seriesLabels: ["Alpha", "Beta"],
  rows: [
    { category: "Speed", values: [80, 55] },
    { category: "Reliability", values: [70, 85] },
    { category: "Comfort", values: [60, 70] },
    { category: "Safety", values: [90, 65] },
    { category: "Efficiency", values: [75, 90] },
  ],
  extraKv: { legend: "bottom" },
});

export const LIQUID_CHART_RADIAL_TEMPLATE = serializeChartFence({
  type: "radial",
  title: "Users by device",
  seriesLabels: ["Users"],
  rows: [
    { category: "Desktop", values: [186] },
    { category: "Mobile", values: [80] },
    { category: "Tablet", values: [120] },
  ],
  extraKv: { legend: "bottom", labels: "category" },
});

/** Default slash chart insert (bar). */
export const LIQUID_CHART_TEMPLATE = LIQUID_CHART_BAR_TEMPLATE;

export const LIQUID_CARD_TEMPLATE = [
  "```card",
  "title: Summary",
  "subtitle: One line context",
  "emoji: 📋",
  "body: Short body for the card.",
  "```",
  "",
].join("\n");

export const LIQUID_CALLOUT_TEMPLATE = [
  "```callout",
  "tone: note",
  "title: Aside",
  "body: Supporting detail for the reader.",
  "```",
  "",
].join("\n");

export const LIQUID_DASHBOARD_TEMPLATE = [
  "```dashboard",
  "title: At a glance",
  "columns: 2",
  "",
  "---",
  "label: Metric",
  "value: 42",
  "tone: success",
  "---",
  "label: Status",
  "value: On track",
  "tone: accent",
  "```",
  "",
].join("\n");
