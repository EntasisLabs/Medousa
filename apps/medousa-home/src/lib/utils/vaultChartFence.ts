/**
 * Chart fence extract / KV splice helpers for vault Configure.
 * Table markdown is preserved byte-for-byte on prop edits.
 */

import type { ChartFenceType } from "$lib/utils/liquidFenceTemplates";

const CHART_FENCE_RE = /```chart\s*\n([\s\S]*?)```/gi;

export interface ChartFenceBlock {
  index: number;
  start: number;
  end: number;
  fullMatch: string;
  body: string;
}

/** Editable chart KV props (table stays untouched). */
export interface ChartFenceKv {
  type: ChartFenceType;
  title: string;
  description: string;
  legend: string;
  labels: string;
  surface: string;
  colors: string;
  seriesMarks: string;
}

export interface ChartFenceParts {
  kv: ChartFenceKv;
  /** Raw KV lines as map (includes keys we don't edit in the sheet). */
  allFields: Record<string, string>;
  /** GFM table (and any trailing non-KV content after first `|` line), preserved. */
  tableMarkdown: string;
}

const CHART_TYPES = new Set<ChartFenceType>([
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

const EDITABLE_KEYS = new Set([
  "type",
  "chart",
  "title",
  "description",
  "subtitle",
  "legend",
  "labels",
  "surface",
  "plot",
  "colors",
  "seriesmarks",
  "series_marks",
]);

export function extractChartFences(source: string): ChartFenceBlock[] {
  const blocks: ChartFenceBlock[] = [];
  const re = new RegExp(CHART_FENCE_RE.source, "gi");
  let match: RegExpExecArray | null;
  let index = 0;
  while ((match = re.exec(source)) !== null) {
    blocks.push({
      index,
      start: match.index,
      end: match.index + match[0].length,
      fullMatch: match[0],
      body: match[1] ?? "",
    });
    index += 1;
  }
  return blocks;
}

function parseKvLines(preamble: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of preamble.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const colon = line.indexOf(":");
    if (colon <= 0) continue;
    const key = line.slice(0, colon).trim().toLowerCase();
    const value = line.slice(colon + 1).trim();
    if (key && value) out[key] = value;
  }
  return out;
}

export function parseChartFenceParts(body: string): ChartFenceParts {
  const normalized = body.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  const preamble: string[] = [];
  const tableLines: string[] = [];
  let inTable = false;

  for (const raw of lines) {
    const stripped = raw.trim();
    if (!inTable && stripped.startsWith("|")) {
      inTable = true;
    }
    if (inTable) {
      tableLines.push(raw);
    } else {
      preamble.push(raw);
    }
  }

  const allFields = parseKvLines(preamble.join("\n"));
  const typeRaw = (allFields.type ?? allFields.chart ?? "bar").trim().toLowerCase();
  const type = CHART_TYPES.has(typeRaw as ChartFenceType)
    ? (typeRaw as ChartFenceType)
    : "bar";

  const kv: ChartFenceKv = {
    type,
    title: allFields.title ?? "",
    description: allFields.description ?? allFields.subtitle ?? "",
    legend: allFields.legend ?? "",
    labels: allFields.labels ?? "",
    surface: allFields.surface ?? allFields.plot ?? "",
    colors: allFields.colors ?? "",
    seriesMarks: allFields.seriesmarks ?? allFields.series_marks ?? "",
  };

  return {
    kv,
    allFields,
    tableMarkdown: tableLines.join("\n"),
  };
}

function serializeEditableKv(kv: ChartFenceKv, preserved: Record<string, string>): string {
  const lines: string[] = [`type: ${kv.type}`];
  if (kv.title.trim()) lines.push(`title: ${kv.title.trim()}`);
  if (kv.description.trim()) lines.push(`description: ${kv.description.trim()}`);
  if (kv.legend.trim()) lines.push(`legend: ${kv.legend.trim()}`);
  if (kv.labels.trim()) lines.push(`labels: ${kv.labels.trim()}`);
  if (kv.surface.trim()) lines.push(`surface: ${kv.surface.trim()}`);
  if (kv.colors.trim()) lines.push(`colors: ${kv.colors.trim()}`);
  if (kv.seriesMarks.trim()) lines.push(`seriesMarks: ${kv.seriesMarks.trim()}`);

  for (const [key, value] of Object.entries(preserved)) {
    const lower = key.toLowerCase();
    if (EDITABLE_KEYS.has(lower)) continue;
    if (!value.trim()) continue;
    lines.push(`${key}: ${value.trim()}`);
  }

  return lines.join("\n");
}

export function serializeChartFenceFromParts(parts: ChartFenceParts, kv: ChartFenceKv): string {
  const head = serializeEditableKv(kv, parts.allFields);
  const table = parts.tableMarkdown.trimEnd();
  const body = table ? `${head}\n\n${table}` : head;
  return `\`\`\`chart\n${body}\n\`\`\``;
}

/** Replace the Nth ```chart fence (0-based), splicing only KV props. */
export function replaceChartFencePropsAt(
  source: string,
  index: number,
  kv: ChartFenceKv,
): string | null {
  const blocks = extractChartFences(source);
  const block = blocks[index];
  if (!block) return null;
  const parts = parseChartFenceParts(block.body);
  const replacement = serializeChartFenceFromParts(parts, kv);
  return source.slice(0, block.start) + replacement + source.slice(block.end);
}
