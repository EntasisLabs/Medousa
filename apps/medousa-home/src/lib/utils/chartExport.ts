/**
 * Per-chart export — PNG / SVG / CSV from a rendered Liquid chart.
 *
 * PNG uses the same html2canvas snapshot path as vault PDF/Word export
 * (`snapshotElementToPng` handles the color-mix scrub). SVG export is
 * best-effort: only pure-SVG marks (pie/donut/radar/radial) produce a single
 * inline SVG; LayerCake Html-layer marks fall back to PNG guidance.
 */

import { snapshotElementToPng } from "$lib/utils/elementSnapshot";
import type { ChartViewModel } from "$lib/liquid/archetypes/organisms/chart/chartModel";
import { chartSeriesColor } from "$lib/liquid/archetypes/organisms/chart/chartModel";

export type ChartExportFormat = "png" | "svg" | "csv";

function slugifyTitle(title: string | undefined): string {
  const base = (title ?? "chart").trim().toLowerCase();
  const slug = base.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return slug || "chart";
}

function triggerDownload(url: string, filename: string): void {
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  try {
    triggerDownload(url, filename);
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 2000);
  }
}

function escapeSvg(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Inline computed colors so the SVG stands alone outside the app theme. */
function bakeComputedStyles(source: SVGSVGElement, clone: SVGSVGElement): void {
  const srcNodes = [source, ...source.querySelectorAll<SVGElement>("*")];
  const cloneNodes = [clone, ...clone.querySelectorAll<SVGElement>("*")];
  const props = ["fill", "stroke", "stop-color", "opacity", "color", "font-size", "font-family"] as const;
  for (let i = 0; i < srcNodes.length; i += 1) {
    const src = srcNodes[i];
    const dst = cloneNodes[i];
    if (!src || !dst) continue;
    const computed = getComputedStyle(src);
    for (const prop of props) {
      const value = computed.getPropertyValue(prop);
      if (value && value !== "none") {
        dst.setAttribute(prop, value);
      }
    }
    // CSS variables drive fills via class styles — bake resolved fill/stroke.
    const fill = computed.fill;
    if (fill && fill !== "none") dst.setAttribute("fill", fill);
    const stroke = computed.stroke;
    if (stroke && stroke !== "none") dst.setAttribute("stroke", stroke);
  }
}

/**
 * Collect the chart's inline SVG (only for marks that render a single pure
 * SVG: pie / donut / radar / radial). Returns null when no suitable SVG.
 */
function collectPureSvg(root: HTMLElement): SVGSVGElement | null {
  const svgs = root.querySelectorAll<SVGSVGElement>("svg");
  if (svgs.length !== 1) return null;
  return svgs[0];
}

export async function exportChartPng(
  root: HTMLElement,
  model: ChartViewModel | null,
): Promise<boolean> {
  const snapshot = await snapshotElementToPng(root);
  if (!snapshot) return false;
  const link = document.createElement("a");
  link.href = snapshot.dataUrl;
  link.download = `${slugifyTitle(model?.title)}.png`;
  link.rel = "noopener";
  document.body.appendChild(link);
  link.click();
  link.remove();
  return true;
}

export async function exportChartSvg(
  root: HTMLElement,
  model: ChartViewModel | null,
): Promise<boolean> {
  const source = collectPureSvg(root);
  if (!source) return false;
  const clone = source.cloneNode(true) as SVGSVGElement;
  bakeComputedStyles(source, clone);
  const box = source.getBoundingClientRect();
  const width = Math.max(1, Math.ceil(box.width || source.clientWidth || 480));
  const height = Math.max(1, Math.ceil(box.height || source.clientHeight || 320));
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("width", String(width));
  clone.setAttribute("height", String(height));
  clone.setAttribute("viewBox", `0 0 ${width} ${height}`);
  const title = model?.title?.trim();
  if (title && !clone.querySelector("title")) {
    const titleEl = document.createElementNS("http://www.w3.org/2000/svg", "title");
    titleEl.textContent = title;
    clone.insertBefore(titleEl, clone.firstChild);
  }
  void escapeSvg; // reserved for title/text baking
  const markup = new XMLSerializer().serializeToString(clone);
  const blob = new Blob([`<?xml version="1.0" encoding="UTF-8"?>\n${markup}`], {
    type: "image/svg+xml",
  });
  downloadBlob(blob, `${slugifyTitle(title)}.svg`);
  return true;
}

/** True when the mark renders a single pure SVG (exportable as .svg). */
export function chartSupportsSvgExport(model: ChartViewModel | null): boolean {
  if (!model) return false;
  return (
    model.type === "pie" ||
    model.type === "donut" ||
    model.type === "radar" ||
    model.type === "radial"
  );
}

function csvEscape(value: string): string {
  if (/[",\n]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export function exportChartCsv(model: ChartViewModel | null): boolean {
  if (!model) return false;
  const rows: string[][] = [];
  if (model.type === "scatter") {
    rows.push(["x", "y", "group"]);
    for (const point of model.points) {
      rows.push([String(point.x), String(point.y), point.group ?? ""]);
    }
  } else if (model.type === "heatmap" && model.matrix) {
    rows.push(["", ...model.matrix.cols]);
    model.matrix.rows.forEach((rowLabel, ri) => {
      rows.push([rowLabel, ...(model.matrix?.values[ri] ?? []).map(String)]);
    });
  } else if (model.type === "pie" || model.type === "donut") {
    const series = model.series[0];
    rows.push(["category", series?.label ?? "value"]);
    model.categories.forEach((category, index) => {
      rows.push([category, String(series?.values[index] ?? "")]);
    });
  } else {
    rows.push(["category", ...model.series.map((s) => s.label)]);
    model.categories.forEach((category, index) => {
      rows.push([category, ...model.series.map((s) => String(s.values[index] ?? ""))]);
    });
  }
  const text = rows.map((row) => row.map(csvEscape).join(",")).join("\n");
  const blob = new Blob([text], { type: "text/csv" });
  downloadBlob(blob, `${slugifyTitle(model.title)}.csv`);
  return true;
}

/** Legend swatch colors baked for standalone contexts (unused hook for SVG parity). */
export function legendColor(model: ChartViewModel, index: number): string {
  return chartSeriesColor(index, model.colors);
}
