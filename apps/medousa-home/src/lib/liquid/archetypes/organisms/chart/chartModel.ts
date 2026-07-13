import type {
  LiquidChartCurve,
  LiquidChartLabelPosition,
  LiquidChartLabels,
  LiquidChartLayout,
  LiquidChartLegend,
  LiquidChartProps,
  LiquidChartSeries,
  LiquidChartTrendDirection,
  LiquidChartType,
} from "$lib/markdown/liquidEmbeds";
import {
  isMarkdownColorId,
  isMarkdownHexColor,
  normalizeMarkdownHexColor,
} from "$lib/utils/vaultMarkdownColors";

const CHART_TYPES = new Set<LiquidChartType>([
  "bar",
  "line",
  "area",
  "pie",
  "donut",
  "radar",
  "radial",
]);

export interface ChartViewModel {
  type: LiquidChartType;
  title: string;
  description: string;
  categories: string[];
  series: LiquidChartSeries[];
  layout: LiquidChartLayout;
  stacked: boolean;
  curve: LiquidChartCurve;
  separator: boolean;
  centerLabel: string;
  centerValue: string;
  trend: string;
  trendDirection: LiquidChartTrendDirection | null;
  caption: string;
  labels: LiquidChartLabels;
  labelPosition: LiquidChartLabelPosition;
  tooltip: boolean;
  legend: LiquidChartLegend;
  interactive: boolean;
  activeKey: string;
  colors: string[];
}

export function resolveChartColor(raw: string, fallbackIndex = 0): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    const n = (fallbackIndex % 5) + 1;
    return `rgb(var(--chart-${n}))`;
  }
  if (isMarkdownColorId(trimmed)) {
    return `rgb(var(--markdown-chart-${trimmed.toLowerCase()}))`;
  }
  const hex = normalizeMarkdownHexColor(trimmed);
  if (hex) return hex;
  if (isMarkdownHexColor(trimmed)) return trimmed;
  // Pass through rgb()/hsl()/named CSS colors from fence overrides
  if (/^(rgb|hsl)a?\(/i.test(trimmed) || /^[a-z]+$/i.test(trimmed)) {
    return trimmed;
  }
  const n = (fallbackIndex % 5) + 1;
  return `rgb(var(--chart-${n}))`;
}

export function chartSeriesColor(index: number, override?: string[]): string {
  if (override?.[index]) return resolveChartColor(override[index], index);
  const n = (index % 5) + 1;
  return `rgb(var(--chart-${n}))`;
}

export function resolveLegend(
  legend: LiquidChartLegend | undefined,
  seriesCount: number,
): "none" | "top" | "bottom" {
  if (legend === false || legend === "none") return "none";
  if (legend === "top") return "top";
  if (legend === true || legend === "bottom") return "bottom";
  return seriesCount > 1 ? "bottom" : "none";
}

function asString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function asBool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function asSeries(raw: unknown): LiquidChartSeries[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((item, i) => {
      if (!item || typeof item !== "object") return null;
      const row = item as Record<string, unknown>;
      const label = asString(row.label) || `Series ${i + 1}`;
      const key = asString(row.key) || `series-${i}`;
      const values = Array.isArray(row.values)
        ? row.values.filter((v): v is number => typeof v === "number" && Number.isFinite(v))
        : [];
      if (!values.length) return null;
      return { key, label, values };
    })
    .filter((s): s is LiquidChartSeries => s !== null);
}

/** Normalize archetype / embed props into a render model. */
export function chartViewModel(props: Record<string, unknown> | LiquidChartProps): ChartViewModel | null {
  const typeRaw = asString((props as LiquidChartProps).type).toLowerCase();
  if (!CHART_TYPES.has(typeRaw as LiquidChartType)) return null;
  const type = typeRaw as LiquidChartType;

  const categories = Array.isArray((props as LiquidChartProps).categories)
    ? (props as LiquidChartProps).categories
        .filter((c): c is string => typeof c === "string")
        .map((c) => c.trim())
        .filter(Boolean)
    : [];
  const series = asSeries((props as LiquidChartProps).series);
  // Radar needs ≥3 axes; radial may be single-arc with 1 category; core types need ≥2.
  const minCats = type === "radar" ? 3 : type === "radial" ? 1 : 2;
  if (categories.length < minCats || series.length < 1) return null;

  const layoutRaw = asString((props as LiquidChartProps).layout).toLowerCase();
  const layout: LiquidChartLayout =
    layoutRaw === "horizontal" ? "horizontal" : "vertical";

  const curveRaw = asString((props as LiquidChartProps).curve).toLowerCase();
  const curve: LiquidChartCurve =
    curveRaw === "linear" || curveRaw === "step" ? curveRaw : "smooth";

  const labelsRaw = asString((props as LiquidChartProps).labels).toLowerCase();
  const labels: LiquidChartLabels =
    labelsRaw === "value" || labelsRaw === "category" || labelsRaw === "both"
      ? labelsRaw
      : "none";

  const labelPosRaw = asString((props as LiquidChartProps).labelPosition).toLowerCase();
  const labelPosition: LiquidChartLabelPosition =
    labelPosRaw === "inside" || labelPosRaw === "outside" ? labelPosRaw : "auto";

  const legendRaw = (props as LiquidChartProps).legend;
  let legend: LiquidChartLegend = series.length > 1 ? "bottom" : "none";
  if (typeof legendRaw === "boolean") legend = legendRaw;
  else if (typeof legendRaw === "string") {
    const v = legendRaw.trim().toLowerCase();
    if (v === "none" || v === "top" || v === "bottom") legend = v;
  }

  const trendDirRaw = asString((props as LiquidChartProps).trendDirection).toLowerCase();
  const trendDirection: LiquidChartTrendDirection | null =
    trendDirRaw === "up" || trendDirRaw === "down" || trendDirRaw === "flat"
      ? trendDirRaw
      : null;

  const colors = Array.isArray((props as LiquidChartProps).colors)
    ? ((props as LiquidChartProps).colors ?? []).filter(
        (c): c is string => typeof c === "string" && c.trim().length > 0,
      )
    : [];

  return {
    type,
    title: asString((props as LiquidChartProps).title),
    description: asString((props as LiquidChartProps).description),
    categories,
    series,
    layout,
    stacked: asBool((props as LiquidChartProps).stacked, false),
    curve,
    separator: asBool((props as LiquidChartProps).separator, true),
    centerLabel: asString((props as LiquidChartProps).centerLabel),
    centerValue: asString((props as LiquidChartProps).centerValue),
    trend: asString((props as LiquidChartProps).trend),
    trendDirection,
    caption: asString((props as LiquidChartProps).caption),
    labels,
    labelPosition,
    tooltip: asBool((props as LiquidChartProps).tooltip, true),
    legend,
    interactive: asBool((props as LiquidChartProps).interactive, true),
    activeKey: asString((props as LiquidChartProps).activeKey),
    colors,
  };
}

export type CartesianRow = Record<string, string | number> & { category: string };

export function toCartesianRows(model: ChartViewModel): CartesianRow[] {
  return model.categories.map((category, i) => {
    const row: CartesianRow = { category };
    for (const s of model.series) {
      row[s.key] = s.values[i] ?? 0;
    }
    return row;
  });
}

export function yMax(model: ChartViewModel): number {
  if (model.stacked) {
    let max = 0;
    for (let i = 0; i < model.categories.length; i++) {
      let sum = 0;
      for (const s of model.series) sum += s.values[i] ?? 0;
      if (sum > max) max = sum;
    }
    return max || 1;
  }
  let max = 0;
  for (const s of model.series) {
    for (const v of s.values) if (v > max) max = v;
  }
  return max || 1;
}

const numberFmt = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 });

export function formatChartNumber(value: number): string {
  if (!Number.isFinite(value)) return "";
  return numberFmt.format(value);
}

/** Resolve `auto` label placement to a concrete inside/outside for a chart type. */
export function resolveLabelPosition(
  model: Pick<ChartViewModel, "type" | "labels" | "labelPosition" | "centerLabel" | "centerValue">,
): "inside" | "outside" | "none" {
  if (model.labels === "none") return "none";
  if (model.labelPosition === "inside" || model.labelPosition === "outside") {
    return model.labelPosition;
  }
  // auto
  if (model.type === "pie") return "outside";
  if (model.type === "donut") {
    // Prefer outside when center chrome is present to avoid collisions.
    if (model.centerLabel || model.centerValue) return "outside";
    return "inside";
  }
  // bar / line / area — end-of-mark labels (treated as "outside" semantically)
  return "outside";
}

export function formatChartLabel(
  labels: LiquidChartLabels,
  category: string,
  value: number,
): string {
  if (labels === "none") return "";
  if (labels === "category") return category;
  if (labels === "both") return `${category} ${formatChartNumber(value)}`;
  return formatChartNumber(value);
}

/** Match activeKey against series key/label or category (case-insensitive). */
export function isActiveKey(
  activeKey: string,
  candidate: { key?: string; label?: string; category?: string },
): boolean {
  const needle = activeKey.trim().toLowerCase();
  if (!needle) return true; // no highlight → everything active
  if (candidate.key?.trim().toLowerCase() === needle) return true;
  if (candidate.label?.trim().toLowerCase() === needle) return true;
  if (candidate.category?.trim().toLowerCase() === needle) return true;
  return false;
}

export function hasActiveHighlight(activeKey: string): boolean {
  return activeKey.trim().length > 0;
}
