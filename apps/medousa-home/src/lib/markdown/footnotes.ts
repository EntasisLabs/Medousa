/**
 * Obsidian-compatible footnote parsing helpers.
 * Syntax: `[^id]` + `[^id]: text`, and inline `^[text]`.
 */

export type FootnoteDef = {
  id: string;
  text: string;
};

export type FootnoteRefHit = {
  /** Original label (`1`, `source`) or generated (`inline-1`). */
  id: string;
  kind: "ref" | "inline";
  /** Inline body when kind === "inline". */
  text?: string;
  index: number;
  length: number;
};

export type FootnotePlan = {
  /** Source with definition lines removed (fences preserved). */
  bodyWithoutDefs: string;
  defs: Map<string, string>;
  /** Display order: first reference encounter, then unused defs. */
  orderedIds: string[];
  numberById: Map<string, number>;
};

const DEF_LINE = /^\[\^([^\]]+)\]:\s?(.*)$/;
const CONT_LINE = /^(?: {2,}|\t)(.*)$/;
const REF_RE = /\[\^([^\]]+)\](?!:)/g;
const INLINE_RE = /\^\[([^\]\n]+)\]/g;

export function footnoteSlug(id: string): string {
  const raw = id
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return raw || "fn";
}

/** Extract `[^id]:` definitions (fence-aware). Returns body without those lines. */
export function extractFootnoteDefinitions(source: string): {
  body: string;
  defs: Map<string, string>;
} {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const defs = new Map<string, string>();
  const out: string[] = [];
  let inFence = false;
  let i = 0;

  while (i < lines.length) {
    const line = lines[i]!;
    const trimmed = line.trimStart();

    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      out.push(line);
      i += 1;
      continue;
    }

    if (inFence) {
      out.push(line);
      i += 1;
      continue;
    }

    const defMatch = DEF_LINE.exec(line);
    if (!defMatch) {
      out.push(line);
      i += 1;
      continue;
    }

    const id = defMatch[1]!.trim();
    const parts = [defMatch[2] ?? ""];
    i += 1;
    while (i < lines.length) {
      const cont = CONT_LINE.exec(lines[i]!);
      if (!cont) break;
      // Stop if next line is another definition.
      if (DEF_LINE.test(lines[i]!)) break;
      parts.push(cont[1] ?? "");
      i += 1;
    }
    if (id && !defs.has(id)) {
      defs.set(id, parts.join("\n").replace(/\s+$/, ""));
    }
  }

  // Drop trailing blank lines left by removed defs at EOF, keep body blanks.
  while (out.length > 0 && out[out.length - 1] === "") {
    // Keep a single trailing newline behavior via join — trim only pure empty tail cluster
    // if the whole file ended in defs. Safer: just join as-is.
    break;
  }

  return { body: out.join("\n"), defs };
}

/**
 * Scan body (no defs) for ref + inline footnote hits in document order.
 * Skips fenced code.
 */
export function collectFootnoteHits(body: string): FootnoteRefHit[] {
  const lines = body.replace(/\r\n/g, "\n").split("\n");
  const hits: FootnoteRefHit[] = [];
  let offset = 0;
  let inFence = false;
  let inlineSeq = 0;

  for (const line of lines) {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      offset += line.length + 1;
      continue;
    }
    if (inFence) {
      offset += line.length + 1;
      continue;
    }

    // Walk the line, preferring inline `^[` then ref `[^` (no overlap in practice).
    let pos = 0;
    while (pos < line.length) {
      const inlineIdx = line.indexOf("^[", pos);
      const refIdx = line.indexOf("[^", pos);
      let next = -1;
      let kind: "inline" | "ref" | null = null;

      if (inlineIdx >= 0 && (refIdx < 0 || inlineIdx <= refIdx)) {
        next = inlineIdx;
        kind = "inline";
      } else if (refIdx >= 0) {
        next = refIdx;
        kind = "ref";
      }

      if (next < 0 || !kind) break;

      if (kind === "inline") {
        INLINE_RE.lastIndex = next;
        const m = INLINE_RE.exec(line);
        if (!m || m.index !== next) {
          pos = next + 2;
          continue;
        }
        inlineSeq += 1;
        const id = `inline-${inlineSeq}`;
        hits.push({
          id,
          kind: "inline",
          text: m[1] ?? "",
          index: offset + m.index,
          length: m[0].length,
        });
        pos = m.index + m[0].length;
        continue;
      }

      REF_RE.lastIndex = next;
      const m = REF_RE.exec(line);
      if (!m || m.index !== next) {
        pos = next + 2;
        continue;
      }
      hits.push({
        id: (m[1] ?? "").trim(),
        kind: "ref",
        index: offset + m.index,
        length: m[0].length,
      });
      pos = m.index + m[0].length;
    }

    offset += line.length + 1;
  }

  return hits;
}

/** Build numbering plan: first-ref order, then unused definitions. */
export function planFootnotes(source: string): FootnotePlan {
  const { body: bodyWithoutDefs, defs } = extractFootnoteDefinitions(source);
  const hits = collectFootnoteHits(bodyWithoutDefs);
  const orderedIds: string[] = [];
  const numberById = new Map<string, number>();
  const inlineText = new Map<string, string>();

  for (const hit of hits) {
    if (numberById.has(hit.id)) continue;
    numberById.set(hit.id, orderedIds.length + 1);
    orderedIds.push(hit.id);
    if (hit.kind === "inline" && hit.text != null) {
      inlineText.set(hit.id, hit.text);
      if (!defs.has(hit.id)) defs.set(hit.id, hit.text);
    }
  }

  for (const id of defs.keys()) {
    if (numberById.has(id)) continue;
    numberById.set(id, orderedIds.length + 1);
    orderedIds.push(id);
  }

  // Attach inline texts into defs for consumers
  for (const [id, text] of inlineText) {
    if (!defs.has(id)) defs.set(id, text);
  }

  return { bodyWithoutDefs, defs, orderedIds, numberById };
}

export function footnoteRefHtml(id: string, n: number): string {
  const slug = footnoteSlug(id);
  return `<sup class="markdown-footnote-ref"><a href="#fn-${slug}" id="fnref-${slug}" data-footnote-ref="${escapeAttr(id)}">${n}</a></sup>`;
}

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Replace refs/inline in bodyWithoutDefs with HTML superscripts (fence-aware). */
export function replaceFootnoteMarkers(
  bodyWithoutDefs: string,
  numberById: Map<string, number>,
): string {
  const lines = bodyWithoutDefs.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let inFence = false;
  let inlineSeq = 0;

  for (const line of lines) {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      out.push(line);
      continue;
    }
    if (inFence) {
      out.push(line);
      continue;
    }

    let result = "";
    let pos = 0;
    while (pos < line.length) {
      const inlineIdx = line.indexOf("^[", pos);
      const refIdx = line.indexOf("[^", pos);
      let next = -1;
      let kind: "inline" | "ref" | null = null;

      if (inlineIdx >= 0 && (refIdx < 0 || inlineIdx <= refIdx)) {
        next = inlineIdx;
        kind = "inline";
      } else if (refIdx >= 0) {
        next = refIdx;
        kind = "ref";
      }

      if (next < 0 || !kind) {
        result += line.slice(pos);
        break;
      }

      result += line.slice(pos, next);

      if (kind === "inline") {
        INLINE_RE.lastIndex = next;
        const m = INLINE_RE.exec(line);
        if (!m || m.index !== next) {
          result += line[next];
          pos = next + 1;
          continue;
        }
        inlineSeq += 1;
        const id = `inline-${inlineSeq}`;
        const n = numberById.get(id) ?? inlineSeq;
        result += footnoteRefHtml(id, n);
        pos = m.index + m[0].length;
        continue;
      }

      REF_RE.lastIndex = next;
      const m = REF_RE.exec(line);
      if (!m || m.index !== next) {
        result += line[next];
        pos = next + 1;
        continue;
      }
      const id = (m[1] ?? "").trim();
      const n = numberById.get(id);
      if (n == null) {
        result += m[0];
      } else {
        result += footnoteRefHtml(id, n);
      }
      pos = m.index + m[0].length;
    }

    out.push(result);
  }

  return out.join("\n");
}

/**
 * Build the footnotes footer HTML.
 * `renderInline` optionally formats definition bodies (bold/links); defaults to escaped text.
 */
export function buildFootnotesSectionHtml(
  plan: FootnotePlan,
  renderInline: (md: string) => string = (md) => escapeHtml(md),
): string {
  if (plan.orderedIds.length === 0) return "";

  const items = plan.orderedIds.map((id) => {
    const slug = footnoteSlug(id);
    const n = plan.numberById.get(id) ?? 0;
    const raw = plan.defs.get(id) ?? "";
    const body = raw ? renderInline(raw) : "";
    return `<li id="fn-${slug}" value="${n}" class="markdown-footnote-item"><span class="markdown-footnote-text">${body}</span> <a class="markdown-footnote-back" href="#fnref-${slug}" aria-label="Back to reference">↩</a></li>`;
  });

  return `\n\n<section class="markdown-footnotes" aria-label="Footnotes"><ol class="markdown-footnotes-list">${items.join("")}</ol></section>\n`;
}
