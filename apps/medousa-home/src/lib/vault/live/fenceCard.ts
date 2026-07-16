/**
 * Split markdown body into prose segments and atomic fenced blocks.
 * Fences stay byte-stable for Live cards → Build jump / serialize.
 */

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

/** Line-start fenced code block (```lang … ```). */
const FENCE_RE = /^```([^\r\n`]*)\r?\n([\s\S]*?)^```[ \t]*\r?$/gm;

export function parseFenceInfo(info: string): { lang: string; meta: string } {
  const trimmed = info.trim();
  if (!trimmed) return { lang: "", meta: "" };
  const space = trimmed.search(/\s/);
  if (space === -1) return { lang: trimmed.toLowerCase(), meta: "" };
  return {
    lang: trimmed.slice(0, space).toLowerCase(),
    meta: trimmed.slice(space).trim(),
  };
}

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
    if (/^(?:title|name|type|chart|description|legend|labels|surface|colors)\s*:/i.test(trimmed)) {
      continue;
    }
    return trimmed.length > max ? `${trimmed.slice(0, max - 1)}…` : trimmed;
  }
  const first = body.trim().split(/\r?\n/)[0]?.trim() ?? "";
  if (!first) return "";
  return first.length > max ? `${first.slice(0, max - 1)}…` : first;
}

export function splitMarkdownSegments(body: string): LiveSegment[] {
  const segments: LiveSegment[] = [];
  const re = new RegExp(FENCE_RE.source, "gm");
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(body)) !== null) {
    const start = match.index;
    const end = start + match[0].length;
    if (start > last) {
      segments.push({
        kind: "prose",
        text: body.slice(last, start),
        start: last,
        end: start,
      });
    }
    const info = match[1] ?? "";
    const { lang } = parseFenceInfo(info);
    segments.push({
      kind: "fence",
      raw: match[0],
      lang,
      body: match[2] ?? "",
      start,
      end,
    });
    last = end;
  }
  if (last < body.length) {
    segments.push({
      kind: "prose",
      text: body.slice(last),
      start: last,
      end: body.length,
    });
  }
  if (segments.length === 0) {
    segments.push({ kind: "prose", text: body, start: 0, end: body.length });
  }
  return segments;
}

export function findFenceOffset(body: string, raw: string): number {
  const idx = body.indexOf(raw);
  return idx;
}
