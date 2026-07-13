import type {
  LiquidChartCurve,
  LiquidChartLabelPosition,
  LiquidChartLabels,
  LiquidChartLayout,
  LiquidChartLegend,
  LiquidChartMatrix,
  LiquidChartPoint,
  LiquidChartProps,
  LiquidChartSeries,
  LiquidChartSeriesMark,
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
  "scatter",
  "combo",
  "heatmap",
]);

export interface ChartViewModel {
  type: LiquidChartType;
  title: string;
  description: string;
  categories: string[];
  series: LiquidChartSeries[];
  points: LiquidChartPoint[];
  matrix: LiquidChartMatrix | null;
  seriesMarks: LiquidChartSeriesMark[];
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
  /** Resolved CSS width, or "" for full bleed. */
  width: string;
  /** Resolved CSS height for cartesian frames, or "" for default. */
  height: string;
  /** Resolved CSS fill for plot wash, or "" to use theme --chart-plot. */
  surface: string;
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

/** Card width → CSS length. Empty / full → "". */
export function resolveChartWidth(raw: string): string {
  const v = raw.trim().toLowerCase();
  if (!v || v === "full" || v === "100%" || v === "auto") return "";
  if (v === "sm") return "16rem";
  if (v === "md") return "22rem";
  if (v === "lg") return "28rem";
  if (/^\d+(\.\d+)?(%|px|rem|em|ch|vw|cqw)$/.test(v)) return v;
  if (/^\d+(\.\d+)?$/.test(v)) return `${v}px`;
  return "";
}

/** Cartesian plot height → CSS length. Empty → "". */
export function resolveChartHeight(raw: string): string {
  const v = raw.trim().toLowerCase();
  if (!v || v === "auto" || v === "md") return "";
  if (v === "sm") return "11rem";
  if (v === "lg") return "18rem";
  if (v === "xl") return "22rem";
  if (/^\d+(\.\d+)?(%|px|rem|em|ch|vh|cqh)$/.test(v)) return v;
  if (/^\d+(\.\d+)?$/.test(v)) return `${v}px`;
  return "";
}

/**
 * Drawing-surface wash for radar plates / polar tracks.
 * Supports: soft | muted | none | gray | blue | blue/25 | #888/40 | gray @ 0.2
 * Slash/at alpha: 0–1 or 0–100 (values > 1 treated as percent).
 */
export function resolveChartSurface(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return "";

  let colorPart = trimmed;
  let alpha: number | null = null;

  const slash = trimmed.match(/^(.+?)\/(\d{1,3}(?:\.\d+)?)\s*$/);
  const at = trimmed.match(/^(.+?)\s*@\s*(\d{1,3}(?:\.\d+)?)\s*$/);
  if (slash) {
    colorPart = slash[1].trim();
    alpha = normalizeSurfaceAlpha(Number(slash[2]));
  } else if (at) {
    colorPart = at[1].trim();
    alpha = normalizeSurfaceAlpha(Number(at[2]));
  }

  const v = colorPart.toLowerCase();
  if (v === "soft" || v === "default") {
    if (alpha == null) return "";
    return surfaceWashFromInk(alpha);
  }
  if (v === "none" || v === "transparent" || v === "off") return "transparent";
  if (v === "muted" || v === "strong") {
    if (alpha == null) return "var(--chart-plot-muted)";
    return surfaceWashFromInk(alpha);
  }
  // gray/grey/neutral/ink — same path as blue (literal RGB triplet token)
  if (v === "gray" || v === "grey" || v === "neutral" || v === "ink") {
    const a = alpha ?? 0.12;
    return `rgb(var(--markdown-chart-gray) / ${a})`;
  }
  if (isMarkdownColorId(v)) {
    const a = alpha ?? 0.16;
    return `rgb(var(--markdown-chart-${v}) / ${a})`;
  }
  const hex = normalizeMarkdownHexColor(colorPart);
  if (hex) {
    const a = alpha ?? 0.16;
    return hexToRgba(hex, a);
  }
  // Avoid opaque CSS named colors (black, silver, …) — wash them.
  if (/^[a-z]+$/i.test(colorPart)) {
    const a = alpha ?? 0.14;
    return `color-mix(in srgb, ${colorPart.toLowerCase()} ${Math.round(a * 100)}%, transparent)`;
  }
  return resolveChartColor(colorPart);
}

/** soft/muted with custom alpha — color-mix avoids nested-var rgb()/alpha SVG bugs. */
function surfaceWashFromInk(alpha: number): string {
  const pct = Math.round(Math.min(1, Math.max(0, alpha)) * 100);
  return `color-mix(in srgb, rgb(var(--chart-plot-ink)) ${pct}%, transparent)`;
}

function normalizeSurfaceAlpha(n: number): number {
  if (!Number.isFinite(n) || n < 0) return 0;
  const a = n > 1 ? n / 100 : n;
  return Math.min(1, Math.max(0, a));
}

/** #RRGGBB → rgba() for SVG fills with alpha. */
function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace("#", "");
  if (h.length !== 6) return `color-mix(in srgb, ${hex} ${Math.round(alpha * 100)}%, transparent)`;
  const r = Number.parseInt(h.slice(0, 2), 16);
  const g = Number.parseInt(h.slice(2, 4), 16);
  const b = Number.parseInt(h.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
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

function asPoints(raw: unknown): LiquidChartPoint[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((item) => {
      if (!item || typeof item !== "object") return null;
      const row = item as Record<string, unknown>;
      const x = typeof row.x === "number" ? row.x : Number(row.x);
      const y = typeof row.y === "number" ? row.y : Number(row.y);
      if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
      const point: LiquidChartPoint = { x, y };
      const group = asString(row.group);
      if (group) point.group = group;
      return point;
    })
    .filter((p): p is LiquidChartPoint => p !== null);
}

function asMatrix(raw: unknown): LiquidChartMatrix | null {
  if (!raw || typeof raw !== "object") return null;
  const row = raw as Record<string, unknown>;
  const rows = Array.isArray(row.rows)
    ? row.rows.filter((r): r is string => typeof r === "string" && r.trim().length > 0)
    : [];
  const cols = Array.isArray(row.cols)
    ? row.cols.filter((c): c is string => typeof c === "string" && c.trim().length > 0)
    : [];
  const values = Array.isArray(row.values) ? row.values : [];
  if (!rows.length || !cols.length || values.length !== rows.length) return null;
  const nums: number[][] = [];
  for (const line of values) {
    if (!Array.isArray(line) || line.length !== cols.length) return null;
    const cells = line.map((v) => (typeof v === "number" ? v : Number(v)));
    if (cells.some((n) => !Number.isFinite(n))) return null;
    nums.push(cells);
  }
  return { rows, cols, values: nums };
}

function defaultSeriesMarks(seriesCount: number, raw: unknown): LiquidChartSeriesMark[] {
  const fromProps = Array.isArray(raw)
    ? raw.filter((m): m is LiquidChartSeriesMark => m === "bar" || m === "line")
    : [];
  const out: LiquidChartSeriesMark[] = [];
  for (let i = 0; i < seriesCount; i++) {
    out.push(fromProps[i] ?? (i === 0 ? "bar" : "line"));
  }
  return out;
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
  const points = asPoints((props as LiquidChartProps).points);
  const matrix = asMatrix((props as LiquidChartProps).matrix);

  if (type === "scatter") {
    if (points.length < 2) return null;
  } else if (type === "heatmap") {
    if (!matrix || matrix.rows.length < 1 || matrix.cols.length < 1) return null;
  } else {
    const minCats = type === "radar" ? 3 : type === "radial" ? 1 : 2;
    if (categories.length < minCats || series.length < 1) return null;
  }

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

  const legendSeriesCount =
    type === "heatmap" ? 0 : type === "scatter" ? Math.max(series.length, 1) : series.length;
  const legendRaw = (props as LiquidChartProps).legend;
  let legend: LiquidChartLegend = legendSeriesCount > 1 ? "bottom" : "none";
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

  const width = resolveChartWidth(asString((props as LiquidChartProps).width));
  const height = resolveChartHeight(asString((props as LiquidChartProps).height));
  const surface = resolveChartSurface(asString((props as LiquidChartProps).surface));
  const seriesMarks = defaultSeriesMarks(series.length, (props as LiquidChartProps).seriesMarks);

  return {
    type,
    title: asString((props as LiquidChartProps).title),
    description: asString((props as LiquidChartProps).description),
    categories,
    series,
    points,
    matrix,
    seriesMarks,
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
    width,
    height,
    surface,
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

/** Max over series whose combo `seriesMarks[i]` matches `want` (non-combo → all series). */
export function yMaxForMarks(
  model: Pick<ChartViewModel, "series" | "seriesMarks" | "type" | "stacked" | "categories">,
  want: LiquidChartSeriesMark,
): number {
  if (model.type !== "combo") return yMax(model as ChartViewModel);

  const marked = model.series
    .map((s, i) => ({ s, i }))
    .filter(({ i }) => {
      const mark = model.seriesMarks[i] ?? (i === 0 ? "bar" : "line");
      return mark === want;
    });

  if (!marked.length) return 0;

  if (model.stacked && want === "bar") {
    let max = 0;
    for (let ci = 0; ci < model.categories.length; ci++) {
      let sum = 0;
      for (const { s } of marked) sum += s.values[ci] ?? 0;
      if (sum > max) max = sum;
    }
    return max || 1;
  }

  let max = 0;
  for (const { s } of marked) {
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
