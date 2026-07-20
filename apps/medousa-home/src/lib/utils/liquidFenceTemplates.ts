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
  | "radial"
  | "scatter"
  | "combo"
  | "heatmap";

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
  extraKv: { legend: "bottom" },
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
  extraKv: { legend: "bottom", surface: "gray/12", width: "md" },
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
  extraKv: { legend: "bottom" },
});

/** Default slash chart insert (bar). */
export const LIQUID_CHART_TEMPLATE = LIQUID_CHART_BAR_TEMPLATE;

export const LIQUID_CHART_AREA_TEMPLATE = serializeChartFence({
  type: "area",
  title: "Trend",
  seriesLabels: ["Desktop", "Mobile"],
  rows: [
    { category: "Jan", values: [186, 80] },
    { category: "Feb", values: [305, 200] },
    { category: "Mar", values: [237, 120] },
  ],
  extraKv: { curve: "smooth", legend: "bottom", colors: "blue, purple" },
});

export const LIQUID_CHART_DONUT_TEMPLATE = serializeChartFence({
  type: "donut",
  title: "Share",
  seriesLabels: ["Share"],
  rows: [
    { category: "Chrome", values: [275] },
    { category: "Safari", values: [200] },
    { category: "Firefox", values: [187] },
  ],
  extraKv: { legend: "bottom", centerLabel: "Total" },
});

export const LIQUID_CHART_SCATTER_TEMPLATE = [
  "```chart",
  "type: scatter",
  "title: Spend vs conversion",
  "legend: bottom",
  "colors: blue, purple",
  "",
  "| X | Y | Cohort |",
  "| - | - | ------ |",
  "| 12 | 40 | Alpha |",
  "| 18 | 55 | Alpha |",
  "| 9 | 22 | Beta |",
  "| 15 | 48 | Beta |",
  "```",
  "",
].join("\n");

export const LIQUID_CHART_COMBO_TEMPLATE = serializeChartFence({
  type: "combo",
  title: "Revenue and growth",
  seriesLabels: ["Revenue", "Growth %"],
  rows: [
    { category: "Jan", values: [120, 4] },
    { category: "Feb", values: [148, 7] },
    { category: "Mar", values: [132, 5] },
  ],
  extraKv: { legend: "bottom", seriesMarks: "bar, line" },
});

export const LIQUID_CHART_HEATMAP_TEMPLATE = [
  "```chart",
  "type: heatmap",
  "title: Activity by hour",
  "colors: blue",
  "",
  "|           | Mon | Tue | Wed |",
  "| --------- | --- | --- | --- |",
  "| Morning   | 2   | 5   | 3   |",
  "| Afternoon | 8   | 6   | 9   |",
  "| Evening   | 4   | 7   | 5   |",
  "```",
  "",
].join("\n");

export const LIQUID_CHART_BY_TYPE: Record<ChartFenceType, string> = {
  bar: LIQUID_CHART_BAR_TEMPLATE,
  line: LIQUID_CHART_LINE_TEMPLATE,
  area: LIQUID_CHART_AREA_TEMPLATE,
  pie: LIQUID_CHART_PIE_TEMPLATE,
  donut: LIQUID_CHART_DONUT_TEMPLATE,
  radar: LIQUID_CHART_RADAR_TEMPLATE,
  radial: LIQUID_CHART_RADIAL_TEMPLATE,
  scatter: LIQUID_CHART_SCATTER_TEMPLATE,
  combo: LIQUID_CHART_COMBO_TEMPLATE,
  heatmap: LIQUID_CHART_HEATMAP_TEMPLATE,
};

export function chartFenceTemplateForType(type: ChartFenceType): string {
  return LIQUID_CHART_BY_TYPE[type] ?? LIQUID_CHART_TEMPLATE;
}

export const CHART_FENCE_TYPE_OPTIONS: Array<{ id: ChartFenceType; label: string }> = [
  { id: "bar", label: "Bar" },
  { id: "line", label: "Line" },
  { id: "area", label: "Area" },
  { id: "pie", label: "Pie" },
  { id: "donut", label: "Donut" },
  { id: "radar", label: "Radar" },
  { id: "radial", label: "Radial" },
  { id: "scatter", label: "Scatter" },
  { id: "combo", label: "Combo" },
  { id: "heatmap", label: "Heatmap" },
];

export const LIQUID_REPORT_TEMPLATE = [
  "```report",
  "title: Q2 growth review",
  "subtitle: North America · weekly pulse",
  "columns: 2",
  "",
  "Opening prose stays full-bleed across the report.",
  "",
  "```chart",
  "type: combo",
  "title: Revenue vs growth",
  "legend: bottom",
  "seriesMarks: bar, line",
  "",
  "| Month | Revenue | Growth % |",
  "| ----- | ------- | -------- |",
  "| Jan   | 120     | 4        |",
  "| Feb   | 148     | 7        |",
  "| Mar   | 132     | 5        |",
  "```",
  "",
  "```chart",
  "type: heatmap",
  "title: Engagement matrix",
  "colors: blue",
  "",
  "|           | Mon | Tue | Wed |",
  "| --------- | --- | --- | --- |",
  "| Morning   | 2   | 5   | 3   |",
  "| Afternoon | 8   | 6   | 9   |",
  "```",
  "",
  "## Deep dive",
  "",
  "More prose after the figures, then another chart if needed.",
  "```",
  "",
].join("\n");

export const LIQUID_SLIDES_TEMPLATE = [
  "```slides",
  "title: Mid-2026 pitch",
  "theme: dusk",
  "columns: 2",
  "",
  "---",
  "label: Title",
  "layout: hero",
  "bg: ember",
  "",
  "# Frontier models",
  "One pick for Live polish",
  "",
  "---",
  "label: Price story",
  "layout: split",
  "bg: ./shots/sky.png",
  "",
  "Prose wraps beside the chart…",
  "",
  "```chart",
  "type: line",
  "title: Price",
  "legend: bottom",
  "",
  "| Month | Price |",
  "| ----- | ----- |",
  "| Jan   | 12    |",
  "| Feb   | 18    |",
  "| Mar   | 15    |",
  "```",
  "```",
  "",
].join("\n");

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

export const LIQUID_TABS_TEMPLATE = [
  "```tabs",
  "title: Getting started",
  "default: Run",
  "",
  "---",
  "label: Install",
  "body: npm install medousa",
  "---",
  "label: Run",
  "body: medousa up",
  "```",
  "",
].join("\n");

export const LIQUID_STEPS_TEMPLATE = [
  "```steps",
  "title: Ship it",
  "",
  "---",
  "label: Build",
  "body: cargo build --release",
  "status: done",
  "---",
  "label: Test",
  "body: Run the smoke suite",
  "status: current",
  "---",
  "label: Deploy",
  "body: Push to production",
  "status: pending",
  "```",
  "",
].join("\n");

export const LIQUID_ACCORDION_TEMPLATE = [
  "```accordion",
  "title: FAQ",
  "",
  "---",
  "label: What is Liquid?",
  "body: Paste-first markdown embeds hydrated by the client.",
  "open: true",
  "---",
  "label: Who owns the vocabulary?",
  "body: The runtime — agents write fences, not HTML.",
  "```",
  "",
].join("\n");

export const LIQUID_CODE_TEMPLATE = [
  "```code",
  "lang: typescript",
  "title: greet.ts",
  "---",
  "export function greet(name: string) {",
  '  return `Hello, ${name}`;',
  "}",
  "```",
  "",
].join("\n");

export const LIQUID_TREE_TEMPLATE = [
  "```tree",
  "title: Project",
  "---",
  "src/",
  "  lib/",
  "    index.ts",
  "  routes/",
  "    +page.svelte",
  "README.md",
  "```",
  "",
].join("\n");

export const LIQUID_KANBAN_TEMPLATE = [
  "```kanban",
  "## Backlog",
  "- [ ] First crumb",
  "",
  "## Doing",
  "- [ ] In progress",
  "",
  "## Done",
  "- [x] Shipped",
  "```",
  "",
].join("\n");

export const LIQUID_CAROUSEL_TEMPLATE = [
  "```carousel",
  "title: Picks",
  "",
  "---",
  "title: Option A",
  "subtitle: Strong all-rounder",
  "emoji: ⭐",
  "body: Short pitch for the first pick.",
  "---",
  "title: Option B",
  "subtitle: Budget friendly",
  "emoji: 💡",
  "body: Short pitch for the second pick.",
  "```",
  "",
].join("\n");

export const LIQUID_ACTIONS_TEMPLATE = [
  "```actions",
  "Dig deeper | dig-deeper",
  "Compare options | compare",
  "```",
  "",
].join("\n");

export const LIQUID_SECTION_TEMPLATE = [
  "```section",
  "title: Section",
  "subtitle: Optional context",
  "---",
  "Supporting paragraph for this section.",
  "```",
  "",
].join("\n");

export const LIQUID_CHIPS_TEMPLATE = [
  "```chips",
  "- Focus | tone: accent | value: focus",
  "Fast | tone: default",
  "Careful | tone: warn",
  "```",
  "",
].join("\n");

export const LIQUID_MEDIA_TEMPLATE = [
  "```media",
  "src: https://example.com/image.png",
  "alt: Diagram",
  "caption: Replace with your image URL or vault path",
  "ratio: 16/9",
  "```",
  "",
].join("\n");

export const LIQUID_CITE_TEMPLATE = [
  "```cite",
  "title: Source title",
  "url: https://example.com",
  "quote: A short excerpt from the source.",
  "source: web",
  "```",
  "",
].join("\n");

export const LIQUID_COMPARE_TEMPLATE = [
  "```compare",
  "title: Head to head",
  "subtitle: Two options",
  "recommendation: Option A",
  "",
  "| | Option A | Option B |",
  "| --- | --- | --- |",
  "| Speed | Fast | Steady |",
  "| Cost | Higher | Lower |",
  "```",
  "",
].join("\n");

export const LIQUID_PLAN_TEMPLATE = [
  "```plan",
  "title: Plan",
  "subtitle: Simple pacing",
  "grouping: day",
  "",
  "---",
  "label: Start here",
  "time: Day 1",
  "emoji: 🚀",
  "body: First move",
  "---",
  "label: Follow through",
  "time: Day 2",
  "emoji: ✅",
  "body: Finish strong",
  "```",
  "",
].join("\n");

export const LIQUID_TIMELINE_TEMPLATE = [
  "```timeline",
  "title: Timeline",
  "subtitle: What happened",
  "granularity: day",
  "",
  "---",
  "ts: Day 1",
  "label: Kickoff",
  "detail: Scoped the work",
  "emoji: 🏁",
  "---",
  "ts: Day 2",
  "label: Shipped",
  "detail: Landed the change",
  "emoji: ✨",
  "```",
  "",
].join("\n");

export const LIQUID_SHORTLIST_TEMPLATE = [
  "```shortlist",
  "title: Shortlist",
  "subtitle: Top picks",
  "criteria: fit · cost · speed",
  "density: comfortable",
  "",
  "---",
  "label: Pick A",
  "summary: Best overall fit",
  "score: 9.1",
  "emoji: 🥇",
  "---",
  "label: Pick B",
  "summary: Strong runner-up",
  "score: 8.4",
  "emoji: 🥈",
  "```",
  "",
].join("\n");

export const LIQUID_DECISION_TEMPLATE = [
  "```decision",
  "title: Which option?",
  "subtitle: Tradeoffs",
  "factors: speed · cost · risk",
  "recommendation: Option A",
  "",
  "---",
  "label: Option A",
  "score: 9.0",
  "pros: Fast | Clear path",
  "cons: Higher cost",
  "---",
  "label: Option B",
  "score: 7.5",
  "pros: Cheaper | Familiar",
  "cons: Slower",
  "```",
  "",
].join("\n");

export const LIQUID_BRIEF_TEMPLATE = [
  "```brief",
  "title: Brief",
  "subtitle: One-page take",
  "tone: research",
  "",
  "---",
  "heading: Why it matters",
  "body: Lead with the point the reader needs.",
  "---",
  "heading: Next step",
  "body: What to do with this.",
  "```",
  "",
].join("\n");
