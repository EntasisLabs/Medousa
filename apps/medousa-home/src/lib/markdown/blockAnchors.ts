/**
 * Obsidian-style block IDs: trailing ` ^id` on a block, and `[[Note#^id]]` fragments.
 * Distinct from footnotes (`[^id]` / `^[…]`).
 */

import type { JSONContent } from "@tiptap/core";

/** Trailing space + caret + id (not followed by `[` — those are footnotes). */
export const TRAILING_BLOCK_ID_RE = /\s+\^([A-Za-z0-9_-]+)\s*$/;

export function normalizeBlockId(raw: string): string | null {
  const t = raw.trim().replace(/^\^/, "");
  if (!t || !/^[A-Za-z0-9_-]+$/.test(t)) return null;
  return t;
}

export function isBlockIdFragment(heading: string): boolean {
  return heading.trim().startsWith("^");
}

export function blockIdFromFragment(heading: string): string | null {
  return normalizeBlockId(heading);
}

export function extractTrailingBlockId(text: string): {
  text: string;
  blockId: string | null;
} {
  const m = TRAILING_BLOCK_ID_RE.exec(text);
  if (!m) return { text, blockId: null };
  return {
    text: text.slice(0, m.index).replace(/\s+$/, ""),
    blockId: m[1] ?? null,
  };
}

export function appendBlockIdMarkdown(
  text: string,
  blockId: string | null | undefined,
): string {
  const id = blockId ? normalizeBlockId(blockId) : null;
  if (!id) return text;
  const base = text.replace(/\s+$/, "");
  return base ? `${base} ^${id}` : `^${id}`;
}

/** Strip trailing ` ^id` from rendered HTML / plain text for Preview display. */
export function stripTrailingBlockIdHtml(html: string): {
  html: string;
  blockId: string | null;
} {
  const plain = html.replace(/<[^>]+>/g, "");
  const extracted = extractTrailingBlockId(plain);
  if (!extracted.blockId) return { html, blockId: null };

  // Prefer stripping from the last text run (after last tag).
  const re = /\s+\^([A-Za-z0-9_-]+)\s*(<\/[^>]+>\s*)*$/;
  const replaced = html.replace(re, (_m, id: string, close = "") => {
    if (id !== extracted.blockId) return _m;
    return close;
  });
  if (replaced !== html) return { html: replaced, blockId: extracted.blockId };

  // Fallback: plain-text strip when no tags.
  return {
    html: extractTrailingBlockId(html).text,
    blockId: extracted.blockId,
  };
}

export function blockAnchorAttrs(blockId: string): string {
  const id = normalizeBlockId(blockId);
  if (!id) return "";
  return ` id="^${id}" data-block-id="${id}"`;
}

/** Peel trailing ` ^id` from TipTap inline JSON content into a blockId attr. */
export function peelBlockIdFromInlineContent(content: JSONContent[] | undefined): {
  content: JSONContent[];
  blockId: string | null;
} {
  if (!content?.length) return { content: content ?? [], blockId: null };
  const next = content.map((n) => ({ ...n }));
  for (let i = next.length - 1; i >= 0; i -= 1) {
    const node = next[i]!;
    if (node.type !== "text" || typeof node.text !== "string") continue;
    const { text, blockId } = extractTrailingBlockId(node.text);
    if (!blockId) return { content, blockId: null };
    if (text) {
      next[i] = { ...node, text };
      return { content: next, blockId };
    }
    next.splice(i, 1);
    return { content: next, blockId };
  }
  return { content, blockId: null };
}
