/**
 * Split markdown body into prose segments and atomic fenced blocks.
 * Fences stay byte-stable for Live cards → Build jump / serialize.
 */

import { scanTopLevelFences } from "./fenceScan";

export type LiveProseSegment = {
  kind: "prose";
  text: string;
  /** Offset in the body string (after frontmatter strip). */
  start: number;
  end: number;
};

export type LiveFenceSegment = {
  kind: "fence";
  /** Full fence including opening/closing ``` lines. */
  raw: string;
  lang: string;
  body: string;
  start: number;
  end: number;
};

export type LiveSegment = LiveProseSegment | LiveFenceSegment;

export { parseFenceInfo } from "./fenceScan";

/** One-line title from common liquid KV (`title:` / `name:`). */
export function detectFenceTitle(body: string): string | null {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const m = /^(?:title|name)\s*:\s*(.+)$/i.exec(trimmed);
    if (m?.[1]?.trim()) return m[1].trim();
    // Stop after first non-KV-looking content for chart tables etc.
    if (trimmed.startsWith("|") || trimmed.startsWith("#")) break;
  }
  return null;
}

/** Collapsed one-line preview for fence cards. */
export function fencePreviewLine(body: string, max = 72): string {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    if (/^(?:title|name|type|chart|description|legend|labels|surface|colors|subtitle|columns)\s*:/i.test(trimmed)) {
      continue;
    }
    return trimmed.length > max ? `${trimmed.slice(0, max - 1)}…` : trimmed;
  }
  const first = body.trim().split(/\r?\n/)[0]?.trim() ?? "";
  if (!first) return "";
  return first.length > max ? `${first.slice(0, max - 1)}…` : first;
}

/**
 * Split body into prose | fence segments.
 * Nested fences (report → chart) stay a single top-level fence atom.
 */
export function splitMarkdownSegments(body: string): LiveSegment[] {
  const normalized = body.replace(/\r\n/g, "\n");
  const top = scanTopLevelFences(normalized);
  const segments: LiveSegment[] = [];
  let last = 0;

  for (const fence of top) {
    if (fence.start > last) {
      segments.push({
        kind: "prose",
        text: normalized.slice(last, fence.start),
        start: last,
        end: fence.start,
      });
    }
    segments.push({
      kind: "fence",
      raw: fence.raw,
      lang: fence.lang,
      body: fence.body,
      start: fence.start,
      end: fence.end,
    });
    last = fence.end;
  }

  if (last < normalized.length) {
    segments.push({
      kind: "prose",
      text: normalized.slice(last),
      start: last,
      end: normalized.length,
    });
  }
  if (segments.length === 0) {
    segments.push({
      kind: "prose",
      text: normalized,
      start: 0,
      end: normalized.length,
    });
  }
  return segments;
}

export function findFenceOffset(body: string, raw: string): number {
  return body.replace(/\r\n/g, "\n").indexOf(raw);
}
