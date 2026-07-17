/**
 * CommonMark-ish fence open/close + depth-aware top-level scan.
 * Nested liquid (```report containing ```chart) stays one outer fence.
 */

export type FenceOpenMatch = {
  ticks: number;
  /** Info string after backticks (lang + meta); empty for bare closers/openers. */
  info: string;
};

export type ScannedTopFence = {
  /** Character offset of opening fence line in the source body. */
  start: number;
  /** Character offset after the fence raw (exclusive). */
  end: number;
  lang: string;
  body: string;
  raw: string;
};

/** Match a line that is only a fence opener/closer (optional info). */
export function matchFenceLine(line: string): FenceOpenMatch | null {
  const m = /^(````*)(.*)$/.exec(line);
  if (!m || (m[1]?.length ?? 0) < 3) return null;
  return {
    ticks: m[1]!.length,
    info: (m[2] ?? "").trim(),
  };
}

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

/**
 * Scan top-level fences with nesting depth:
 * - depth 0 + fence line → open
 * - depth > 0 + info string → nested open
 * - depth > 0 + bare ticks (≥ open ticks) → close / pop
 */
export function scanTopLevelFences(source: string): ScannedTopFence[] {
  const normalized = source.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");

  // Character offset of each line start in `normalized`.
  const lineStarts: number[] = new Array(lines.length);
  {
    let pos = 0;
    for (let i = 0; i < lines.length; i++) {
      lineStarts[i] = pos;
      pos += lines[i]!.length;
      if (i < lines.length - 1) pos += 1; // the \n between lines
    }
  }

  const fences: ScannedTopFence[] = [];
  type Frame = { ticks: number; openLine: number; lang: string };
  const stack: Frame[] = [];

  for (let i = 0; i < lines.length; i++) {
    const fence = matchFenceLine(lines[i]!);
    if (!fence) continue;

    if (stack.length === 0) {
      const { lang } = parseFenceInfo(fence.info);
      stack.push({ ticks: fence.ticks, openLine: i, lang });
      continue;
    }

    const top = stack[stack.length - 1]!;
    if (fence.info) {
      const { lang } = parseFenceInfo(fence.info);
      stack.push({ ticks: fence.ticks, openLine: i, lang });
      continue;
    }

    if (fence.ticks < top.ticks) continue;
    stack.pop();
    if (stack.length > 0) continue;

    const openLine = top.openLine;
    const closeLine = i;
    const start = lineStarts[openLine]!;
    const closeLineStart = lineStarts[closeLine]!;
    const end = closeLineStart + lines[closeLine]!.length;
    const raw = normalized.slice(start, end);
    const body = lines.slice(openLine + 1, closeLine).join("\n");

    fences.push({
      start,
      end,
      lang: top.lang,
      body,
      raw,
    });
  }

  return fences;
}
