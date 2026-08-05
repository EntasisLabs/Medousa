import { escapeAttr, escapeHtml } from "../escape.js";
import type {
  LiquidChartMatrix,
  LiquidChartPoint,
  LiquidChartProps,
  LiquidChartSeries,
} from "../liquidEmbeds.js";

const WIDTH = 560;
const HEIGHT = 230;
const PLOT_LEFT = 42;
const PLOT_RIGHT = 16;
const PLOT_TOP = 14;
const PLOT_BOTTOM = 34;
const PLOT_WIDTH = WIDTH - PLOT_LEFT - PLOT_RIGHT;
const PLOT_HEIGHT = HEIGHT - PLOT_TOP - PLOT_BOTTOM;

const DEFAULT_COLORS = [
  "#6c78ff",
  "#46a77b",
  "#d39a35",
  "#d66565",
  "#55a5c7",
  "#9a70d6",
  "#cf6fa5",
  "#7d9b48",
];

function finite(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function safeColor(value: string | undefined, fallback: string): string {
  if (!value) return fallback;
  const color = value.trim();
  if (
    /^#[\da-f]{3,8}$/i.test(color) ||
    /^(?:rgb|hsl)a?\([\d\s.,%/-]+\)$/i.test(color) ||
    /^var\(--[\w-]+\)$/.test(color)
  ) {
    return color;
  }
  return fallback;
}

function palette(props: LiquidChartProps): string[] {
  const requested = props.colors ?? [];
  const count = Math.max(props.series?.length ?? 0, props.categories?.length ?? 0, 1);
  return Array.from({ length: count }, (_, index) =>
    safeColor(requested[index], DEFAULT_COLORS[index % DEFAULT_COLORS.length]!),
  );
}

function seriesValues(series: LiquidChartSeries[], categories: string[]): number[][] {
  return series.map((entry) => categories.map((_, index) => finite(entry.values[index])));
}

function range(values: number[]): { min: number; max: number } {
  if (values.length === 0) return { min: 0, max: 1 };
  let min = Math.min(0, ...values);
  let max = Math.max(0, ...values);
  if (min === max) max = min + 1;
  const padding = (max - min) * 0.08;
  min = min < 0 ? min - padding : 0;
  max += padding;
  return { min, max };
}

function scaleY(value: number, bounds: { min: number; max: number }): number {
  return PLOT_TOP + ((bounds.max - value) / (bounds.max - bounds.min)) * PLOT_HEIGHT;
}

function compactNumber(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1_000_000) return `${(value / 1_000_000).toFixed(abs >= 10_000_000 ? 0 : 1)}m`;
  if (abs >= 1_000) return `${(value / 1_000).toFixed(abs >= 10_000 ? 0 : 1)}k`;
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function cartesianFrame(
  categories: string[],
  bounds: { min: number; max: number },
): string {
  const grid = Array.from({ length: 5 }, (_, index) => {
    const ratio = index / 4;
    const y = PLOT_TOP + ratio * PLOT_HEIGHT;
    const value = bounds.max - ratio * (bounds.max - bounds.min);
    return `<line class="medousa-liquid__chart-grid" x1="${PLOT_LEFT}" y1="${y.toFixed(2)}" x2="${WIDTH - PLOT_RIGHT}" y2="${y.toFixed(2)}"></line><text x="${PLOT_LEFT - 6}" y="${(y + 3).toFixed(2)}" text-anchor="end">${escapeHtml(compactNumber(value))}</text>`;
  }).join("");

  const labels = categories
    .map((label, index) => {
      if (categories.length > 9 && index % Math.ceil(categories.length / 8) !== 0) return "";
      const x = PLOT_LEFT + ((index + 0.5) / Math.max(categories.length, 1)) * PLOT_WIDTH;
      const clipped = label.length > 14 ? `${label.slice(0, 13)}…` : label;
      return `<text x="${x.toFixed(2)}" y="${HEIGHT - 10}" text-anchor="middle">${escapeHtml(clipped)}</text>`;
    })
    .join("");
  return grid + labels;
}

function renderVerticalSeries(props: LiquidChartProps, colors: string[]): string {
  const categories = props.categories ?? [];
  const series = props.series ?? [];
  const values = seriesValues(series, categories);
  const flattened = values.flat();
  const bounds = range(
    props.stacked
      ? categories.flatMap((_, categoryIndex) => {
          const column = values.map((row) => row[categoryIndex] ?? 0);
          return [
            column.filter((value) => value > 0).reduce((sum, value) => sum + value, 0),
            column.filter((value) => value < 0).reduce((sum, value) => sum + value, 0),
          ];
        })
      : flattened,
  );
  const frame = cartesianFrame(categories, bounds);
  const zeroY = scaleY(0, bounds);
  const slot = PLOT_WIDTH / Math.max(categories.length, 1);
  const marks: string[] = [];
  const marksForSeries = props.seriesMarks ?? [];

  series.forEach((entry, seriesIndex) => {
    const mark = props.type === "combo" ? marksForSeries[seriesIndex] ?? (seriesIndex === 0 ? "bar" : "line") : props.type;
    if (mark === "line" || mark === "area") {
      const points = categories.map((_, categoryIndex) => {
        const x = PLOT_LEFT + (categoryIndex + 0.5) * slot;
        const y = scaleY(values[seriesIndex]?.[categoryIndex] ?? 0, bounds);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      });
      if (mark === "area" && points.length > 1) {
        const firstX = PLOT_LEFT + slot * 0.5;
        const lastX = PLOT_LEFT + (categories.length - 0.5) * slot;
        marks.push(
          `<polygon points="${firstX.toFixed(2)},${zeroY.toFixed(2)} ${points.join(" ")} ${lastX.toFixed(2)},${zeroY.toFixed(2)}" fill="${escapeAttr(colors[seriesIndex]!)}" opacity=".18"></polygon>`,
        );
      }
      marks.push(
        `<polyline points="${points.join(" ")}" fill="none" stroke="${escapeAttr(colors[seriesIndex]!)}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"></polyline>`,
      );
      points.forEach((point) => {
        const [x, y] = point.split(",");
        marks.push(`<circle cx="${x}" cy="${y}" r="3" fill="${escapeAttr(colors[seriesIndex]!)}"></circle>`);
      });
      return;
    }

    let positiveBase = 0;
    let negativeBase = 0;
    categories.forEach((_, categoryIndex) => {
      const value = values[seriesIndex]?.[categoryIndex] ?? 0;
      let from = 0;
      let to = value;
      if (props.stacked) {
        const preceding = values.slice(0, seriesIndex).map((row) => row[categoryIndex] ?? 0);
        positiveBase = preceding.filter((item) => item > 0).reduce((sum, item) => sum + item, 0);
        negativeBase = preceding.filter((item) => item < 0).reduce((sum, item) => sum + item, 0);
        from = value >= 0 ? positiveBase : negativeBase;
        to = from + value;
      }
      const y1 = scaleY(from, bounds);
      const y2 = scaleY(to, bounds);
      const groupWidth = slot * 0.72;
      const barWidth = props.stacked ? groupWidth : groupWidth / Math.max(series.length, 1);
      const x = props.stacked
        ? PLOT_LEFT + categoryIndex * slot + (slot - groupWidth) / 2
        : PLOT_LEFT + categoryIndex * slot + (slot - groupWidth) / 2 + seriesIndex * barWidth;
      marks.push(
        `<rect x="${x.toFixed(2)}" y="${Math.min(y1, y2).toFixed(2)}" width="${Math.max(1, barWidth - 1.5).toFixed(2)}" height="${Math.max(1, Math.abs(y2 - y1)).toFixed(2)}" rx="2" fill="${escapeAttr(colors[seriesIndex]!)}"></rect>`,
      );
    });
  });

  return frame + marks.join("");
}

function renderHorizontalBars(props: LiquidChartProps, colors: string[]): string {
  const categories = props.categories ?? [];
  const series = props.series ?? [];
  const values = seriesValues(series, categories);
  const max = Math.max(1, ...values.flat().map((value) => Math.abs(value)));
  const rowHeight = PLOT_HEIGHT / Math.max(categories.length, 1);
  const bars: string[] = [];
  categories.forEach((category, categoryIndex) => {
    const clipped = category.length > 13 ? `${category.slice(0, 12)}…` : category;
    bars.push(`<text x="${PLOT_LEFT - 6}" y="${(PLOT_TOP + (categoryIndex + 0.55) * rowHeight).toFixed(2)}" text-anchor="end">${escapeHtml(clipped)}</text>`);
    series.forEach((_, seriesIndex) => {
      const value = Math.max(0, values[seriesIndex]?.[categoryIndex] ?? 0);
      const barHeight = (rowHeight * 0.72) / Math.max(series.length, 1);
      const y = PLOT_TOP + categoryIndex * rowHeight + rowHeight * 0.14 + seriesIndex * barHeight;
      const width = (value / max) * PLOT_WIDTH;
      bars.push(`<rect x="${PLOT_LEFT}" y="${y.toFixed(2)}" width="${Math.max(1, width).toFixed(2)}" height="${Math.max(2, barHeight - 1).toFixed(2)}" rx="2" fill="${escapeAttr(colors[seriesIndex]!)}"></rect>`);
    });
  });
  return bars.join("");
}

function polarPoint(cx: number, cy: number, radius: number, angle: number): [number, number] {
  return [cx + Math.cos(angle) * radius, cy + Math.sin(angle) * radius];
}

function arcPath(cx: number, cy: number, radius: number, start: number, end: number, inner = 0): string {
  const [x1, y1] = polarPoint(cx, cy, radius, start);
  const [x2, y2] = polarPoint(cx, cy, radius, end);
  const large = end - start > Math.PI ? 1 : 0;
  if (inner <= 0) {
    return `M ${cx} ${cy} L ${x1.toFixed(2)} ${y1.toFixed(2)} A ${radius} ${radius} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)} Z`;
  }
  const [ix2, iy2] = polarPoint(cx, cy, inner, end);
  const [ix1, iy1] = polarPoint(cx, cy, inner, start);
  return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${radius} ${radius} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)} L ${ix2.toFixed(2)} ${iy2.toFixed(2)} A ${inner} ${inner} 0 ${large} 0 ${ix1.toFixed(2)} ${iy1.toFixed(2)} Z`;
}

function renderPie(props: LiquidChartProps, colors: string[]): string {
  const values = (props.series?.[0]?.values ?? []).slice(0, props.categories.length).map((value) => Math.max(0, finite(value)));
  const total = values.reduce((sum, value) => sum + value, 0) || 1;
  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;
  const radius = 87;
  const inner = props.type === "donut" || props.type === "radial" ? 48 : 0;
  let angle = -Math.PI / 2;
  const paths = values.map((value, index) => {
    const next = angle + (value / total) * Math.PI * 2;
    const path = `<path d="${arcPath(cx, cy, radius, angle, next, inner)}" fill="${escapeAttr(colors[index]!)}" stroke="var(--liquid-bg-raised)" stroke-width="2"></path>`;
    angle = next;
    return path;
  });
  if (inner > 0 && (props.centerLabel || props.centerValue)) {
    paths.push(`<text x="${cx}" y="${cy - 3}" text-anchor="middle" style="font-size:15px;fill:var(--liquid-fg);font-weight:700">${escapeHtml(props.centerValue ?? "")}</text>`);
    paths.push(`<text x="${cx}" y="${cy + 15}" text-anchor="middle">${escapeHtml(props.centerLabel ?? "")}</text>`);
  }
  return paths.join("");
}

function renderRadar(props: LiquidChartProps, colors: string[]): string {
  const categories = props.categories ?? [];
  const series = props.series ?? [];
  const count = Math.max(categories.length, 3);
  const cx = WIDTH / 2;
  const cy = HEIGHT / 2 + 3;
  const radius = 82;
  const max = Math.max(1, ...series.flatMap((entry) => entry.values.map((value) => Math.abs(finite(value)))));
  const grid = [0.25, 0.5, 0.75, 1]
    .map((ratio) => {
      const points = Array.from({ length: count }, (_, index) => {
        const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
        const [x, y] = polarPoint(cx, cy, radius * ratio, angle);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      });
      return `<polygon points="${points.join(" ")}" fill="none" class="medousa-liquid__chart-grid"></polygon>`;
    })
    .join("");
  const labels = categories
    .map((label, index) => {
      const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
      const [x, y] = polarPoint(cx, cy, radius + 14, angle);
      return `<text x="${x.toFixed(2)}" y="${(y + 3).toFixed(2)}" text-anchor="middle">${escapeHtml(label)}</text>`;
    })
    .join("");
  const polygons = series
    .map((entry, seriesIndex) => {
      const points = Array.from({ length: count }, (_, index) => {
        const ratio = Math.max(0, finite(entry.values[index])) / max;
        const angle = -Math.PI / 2 + (index / count) * Math.PI * 2;
        const [x, y] = polarPoint(cx, cy, radius * ratio, angle);
        return `${x.toFixed(2)},${y.toFixed(2)}`;
      });
      return `<polygon points="${points.join(" ")}" fill="${escapeAttr(colors[seriesIndex]!)}" fill-opacity=".13" stroke="${escapeAttr(colors[seriesIndex]!)}" stroke-width="2"></polygon>`;
    })
    .join("");
  return grid + labels + polygons;
}

function renderScatter(points: LiquidChartPoint[], colors: string[]): string {
  const safe = points.map((point) => ({ x: finite(point.x), y: finite(point.y), group: point.group ?? "Values" }));
  const xBounds = range(safe.map((point) => point.x));
  const yBounds = range(safe.map((point) => point.y));
  const groups = Array.from(new Set(safe.map((point) => point.group)));
  const grid = cartesianFrame([], yBounds);
  return grid + safe.map((point) => {
    const x = PLOT_LEFT + ((point.x - xBounds.min) / (xBounds.max - xBounds.min)) * PLOT_WIDTH;
    const y = scaleY(point.y, yBounds);
    const color = colors[Math.max(0, groups.indexOf(point.group)) % colors.length]!;
    return `<circle cx="${x.toFixed(2)}" cy="${y.toFixed(2)}" r="4" fill="${escapeAttr(color)}" opacity=".86"></circle>`;
  }).join("");
}

function renderHeatmap(matrix: LiquidChartMatrix, colors: string[]): string {
  const rows = matrix.rows ?? [];
  const cols = matrix.cols ?? [];
  const values = matrix.values?.flat().map((value) => finite(value)) ?? [];
  const min = values.length ? Math.min(...values) : 0;
  const max = values.length ? Math.max(...values) : 1;
  const cellWidth = PLOT_WIDTH / Math.max(cols.length, 1);
  const cellHeight = PLOT_HEIGHT / Math.max(rows.length, 1);
  const base = colors[0] ?? DEFAULT_COLORS[0]!;
  const cells: string[] = [];
  rows.forEach((row, rowIndex) => {
    cells.push(`<text x="${PLOT_LEFT - 6}" y="${(PLOT_TOP + (rowIndex + 0.56) * cellHeight).toFixed(2)}" text-anchor="end">${escapeHtml(row)}</text>`);
    cols.forEach((col, colIndex) => {
      if (rowIndex === 0) {
        cells.push(`<text x="${(PLOT_LEFT + (colIndex + 0.5) * cellWidth).toFixed(2)}" y="${HEIGHT - 10}" text-anchor="middle">${escapeHtml(col)}</text>`);
      }
      const value = finite(matrix.values?.[rowIndex]?.[colIndex]);
      const opacity = max === min ? 0.65 : 0.15 + ((value - min) / (max - min)) * 0.85;
      cells.push(`<rect x="${(PLOT_LEFT + colIndex * cellWidth + 1).toFixed(2)}" y="${(PLOT_TOP + rowIndex * cellHeight + 1).toFixed(2)}" width="${Math.max(1, cellWidth - 2).toFixed(2)}" height="${Math.max(1, cellHeight - 2).toFixed(2)}" rx="2" fill="${escapeAttr(base)}" opacity="${opacity.toFixed(3)}"><title>${escapeHtml(`${row} · ${col}: ${compactNumber(value)}`)}</title></rect>`);
    });
  });
  return cells.join("");
}

function legend(props: LiquidChartProps, colors: string[]): string {
  const entries = props.type === "pie" || props.type === "donut" || props.type === "radial"
    ? props.categories.map((label, index) => ({ label, color: colors[index]! }))
    : props.type === "scatter" && props.points?.length
      ? Array.from(new Set(props.points.map((point) => point.group ?? "Values"))).map((label, index) => ({ label, color: colors[index]! }))
      : props.series.map((entry, index) => ({ label: entry.label, color: colors[index]! }));
  if (props.legend === false || props.legend === "none" || entries.length < 2) return "";
  return `<div class="medousa-liquid__legend">${entries.map((entry) => `<span class="medousa-liquid__legend-item"><span class="medousa-liquid__legend-swatch" style="--swatch:${escapeAttr(entry.color)}"></span>${escapeHtml(entry.label)}</span>`).join("")}</div>`;
}

/** Render the chart body with dependency-free, responsive SVG. */
export function renderLiquidChart(props: LiquidChartProps): string | null {
  const colors = palette(props);
  let marks = "";
  if (props.type === "scatter" && props.points?.length) {
    marks = renderScatter(props.points, colors);
  } else if (props.type === "heatmap" && props.matrix?.rows?.length && props.matrix.cols.length) {
    marks = renderHeatmap(props.matrix, colors);
  } else if (props.type === "pie" || props.type === "donut" || props.type === "radial") {
    if (!props.categories?.length || !props.series?.[0]?.values?.length) return null;
    marks = renderPie(props, colors);
  } else if (props.type === "radar") {
    if (!props.categories?.length || !props.series?.length) return null;
    marks = renderRadar(props, colors);
  } else {
    if (!props.categories?.length || !props.series?.length) return null;
    marks = props.layout === "horizontal" && props.type === "bar"
      ? renderHorizontalBars(props, colors)
      : renderVerticalSeries(props, colors);
  }

  const label = props.title || props.description || `${props.type} chart`;
  const trendClass = props.trendDirection ? ` medousa-liquid__trend--${props.trendDirection}` : "";
  return [
    `<div class="medousa-liquid__chart">`,
    `<svg class="medousa-liquid__chart-svg" viewBox="0 0 ${WIDTH} ${HEIGHT}" role="img" aria-label="${escapeAttr(label)}">${marks}</svg>`,
    legend(props, colors),
    props.trend ? `<div class="medousa-liquid__trend${trendClass}">${escapeHtml(props.trend)}</div>` : "",
    props.caption ? `<p class="medousa-liquid__caption">${escapeHtml(props.caption)}</p>` : "",
    `</div>`,
  ].join("");
}
