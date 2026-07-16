/** Resolve Configure indices for fences mounted as isolated TipTap atoms. */

import { extractChartFences } from "$lib/utils/vaultChartFence";

function normalizeFence(raw: string): string {
  return raw.replace(/\r\n/g, "\n").trim().replace(/\n+$/, "");
}

export function findChartFenceIndex(documentMarkdown: string, fenceRaw: string): number {
  const target = normalizeFence(fenceRaw);
  if (!target) return -1;
  const blocks = extractChartFences(documentMarkdown);
  return blocks.findIndex((b) => normalizeFence(b.fullMatch) === target);
}

/**
 * Map a Configure click to a document chart index.
 * Standalone chart atoms match `hostRaw` directly; nested charts inside
 * report/etc use the local `data-edit-chart-index` within that host fragment.
 */
export function resolveLiveChartIndex(
  documentMarkdown: string,
  hostRaw: string,
  localIndex: number,
): number {
  const direct = findChartFenceIndex(documentMarkdown, hostRaw);
  if (direct >= 0) return direct;
  const localCharts = extractChartFences(hostRaw);
  const target = localCharts[localIndex];
  if (!target) return -1;
  const global = extractChartFences(documentMarkdown);
  const byMatch = global.findIndex(
    (g) => normalizeFence(g.fullMatch) === normalizeFence(target.fullMatch),
  );
  if (byMatch >= 0) return byMatch;
  return global.findIndex((g) => g.body.trim() === target.body.trim());
}

export function findViewFenceIndex(documentMarkdown: string, fenceRaw: string): number {
  const target = normalizeFence(fenceRaw);
  if (!target) return -1;
  const re = /```medousa-view\s*\n([\s\S]*?)```/gi;
  let match: RegExpExecArray | null;
  let index = 0;
  while ((match = re.exec(documentMarkdown)) !== null) {
    if (normalizeFence(match[0]) === target) return index;
    index += 1;
  }
  return -1;
}
