/** Find-in-note helpers for textarea selection and preview scroll. */

export interface FindMatch {
  start: number;
  end: number;
}

const MARK_CLASS = "vault-find-mark";
export const FIND_HIGHLIGHT_ALL = "medousa-vault-find";
export const FIND_HIGHLIGHT_ACTIVE = "medousa-vault-find-active";

export const VAULT_FIND_INPUT_ID = "vault-find-input";

export interface FindMatchOptions {
  caseSensitive?: boolean;
}

export function findMatches(
  text: string,
  query: string,
  options: FindMatchOptions = {},
): FindMatch[] {
  const needle = query.trim();
  if (!needle) return [];

  const caseSensitive = Boolean(options.caseSensitive);
  const haystack = caseSensitive ? text : text.toLowerCase();
  const searchNeedle = caseSensitive ? needle : needle.toLowerCase();
  const matches: FindMatch[] = [];
  let index = 0;

  while (index < haystack.length) {
    const found = haystack.indexOf(searchNeedle, index);
    if (found === -1) break;
    matches.push({ start: found, end: found + needle.length });
    index = found + Math.max(searchNeedle.length, 1);
  }

  return matches;
}

export function replaceFindMatch(
  content: string,
  match: FindMatch,
  replacement: string,
): { content: string; selectionStart: number; selectionEnd: number } {
  const next = `${content.slice(0, match.start)}${replacement}${content.slice(match.end)}`;
  const end = match.start + replacement.length;
  return { content: next, selectionStart: match.start, selectionEnd: end };
}

export function replaceAllFindMatches(
  content: string,
  query: string,
  replacement: string,
  options: FindMatchOptions = {},
): { content: string; selectionStart: number; selectionEnd: number; count: number } {
  const matches = findMatches(content, query, options);
  if (matches.length === 0) {
    return {
      content,
      selectionStart: 0,
      selectionEnd: 0,
      count: 0,
    };
  }
  let next = content;
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    const match = matches[index]!;
    next = `${next.slice(0, match.start)}${replacement}${next.slice(match.end)}`;
  }
  const last = matches[matches.length - 1]!;
  const delta = (replacement.length - (last.end - last.start)) * (matches.length - 1);
  const end = last.start + delta + replacement.length;
  return {
    content: next,
    selectionStart: Math.max(0, end - replacement.length),
    selectionEnd: end,
    count: matches.length,
  };
}

export function findInputHasFocus(): boolean {
  if (typeof document === "undefined") return false;
  return document.activeElement?.id === VAULT_FIND_INPUT_ID;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function buildTextareaFindBackdropHtml(
  text: string,
  matches: FindMatch[],
  activeIndex: number,
): string {
  if (matches.length === 0) return escapeHtml(text);

  const safeIndex =
    ((activeIndex % matches.length) + matches.length) % matches.length;
  let html = "";
  let cursor = 0;

  for (let index = 0; index < matches.length; index += 1) {
    const match = matches[index]!;
    if (match.start > cursor) {
      html += escapeHtml(text.slice(cursor, match.start));
    }
    const className =
      index === safeIndex
        ? `${MARK_CLASS} vault-find-mark-active`
        : MARK_CLASS;
    html += `<mark class="${className}">${escapeHtml(text.slice(match.start, match.end))}</mark>`;
    cursor = match.end;
  }

  if (cursor < text.length) {
    html += escapeHtml(text.slice(cursor));
  }

  return html;
}

export function syncTextareaFindScroll(
  textarea: HTMLTextAreaElement,
  backdrop: HTMLElement,
) {
  backdrop.scrollTop = textarea.scrollTop;
  backdrop.scrollLeft = textarea.scrollLeft;
}

export function updateTextareaFindBackdrop(
  textarea: HTMLTextAreaElement,
  backdrop: HTMLElement | null,
  text: string,
  matches: FindMatch[],
  activeIndex: number,
  query: string,
) {
  if (!backdrop) return;

  if (!query.trim() || matches.length === 0) {
    backdrop.innerHTML = "";
    backdrop.hidden = true;
    textarea.classList.remove("vault-find-editor-input--active");
    return;
  }

  backdrop.hidden = false;
  backdrop.innerHTML = buildTextareaFindBackdropHtml(text, matches, activeIndex);
  textarea.classList.add("vault-find-editor-input--active");
  syncTextareaFindScroll(textarea, backdrop);
}

export function clearTextareaFindBackdrop(
  textarea: HTMLTextAreaElement | null,
  backdrop: HTMLElement | null,
) {
  if (textarea) {
    textarea.classList.remove("vault-find-editor-input--active");
  }
  if (!backdrop) return;
  backdrop.innerHTML = "";
  backdrop.hidden = true;
}

export function revealTextareaMatch(
  textarea: HTMLTextAreaElement,
  match: FindMatch | null,
  options?: { focus?: boolean },
) {
  if (!match) return;

  const before = textarea.value.slice(0, match.start);
  const lineIndex = before.split("\n").length - 1;
  const style = getComputedStyle(textarea);
  const lineHeight =
    Number.parseFloat(style.lineHeight) ||
    Number.parseFloat(style.fontSize) * 1.5 ||
    20;
  const targetTop = lineIndex * lineHeight - textarea.clientHeight / 3;
  textarea.scrollTop = Math.max(0, targetTop);

  if (options?.focus) {
    textarea.focus();
    textarea.setSelectionRange(match.start, match.end);
    return;
  }

  if (findInputHasFocus()) return;

  textarea.setSelectionRange(match.start, match.end);
}

export function clearPreviewFindHighlights() {
  if (typeof CSS !== "undefined" && "highlights" in CSS) {
    CSS.highlights.delete(FIND_HIGHLIGHT_ALL);
    CSS.highlights.delete(FIND_HIGHLIGHT_ACTIVE);
  }
}

export function clearFindHighlights(root?: HTMLElement | null) {
  clearPreviewFindHighlights();
  if (!root) return;
  root.querySelectorAll(`mark.${MARK_CLASS}`).forEach((mark) => {
    const parent = mark.parentNode;
    if (!parent) return;
    while (mark.firstChild) {
      parent.insertBefore(mark.firstChild, mark);
    }
    parent.removeChild(mark);
    parent.normalize();
  });
}

interface TextSegment {
  node: Text;
  start: number;
  end: number;
}

function collectTextSegments(root: HTMLElement): TextSegment[] {
  const segments: TextSegment[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!parent) return NodeFilter.FILTER_REJECT;
      // Skip liquid mounts / chart SVG so find indices stay aligned with editor source.
      if (
        parent.closest(
          "code, pre, script, style, textarea, .liquid-md-host, [data-liquid-embed], .liquid-chart svg",
        )
      ) {
        return NodeFilter.FILTER_REJECT;
      }
      return node.textContent?.length ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
    },
  });

  let offset = 0;
  let current = walker.nextNode();
  while (current) {
    if (current instanceof Text) {
      const length = current.data.length;
      segments.push({ node: current, start: offset, end: offset + length });
      offset += length;
    }
    current = walker.nextNode();
  }

  return segments;
}

export function renderedPlainText(root: HTMLElement): string {
  return collectTextSegments(root)
    .map((segment) => segment.node.data)
    .join("");
}

function rangeForMatch(root: HTMLElement, start: number, end: number): Range | null {
  const segments = collectTextSegments(root);
  let startNode: Text | null = null;
  let startOffset = 0;
  let endNode: Text | null = null;
  let endOffset = 0;

  for (const segment of segments) {
    if (!startNode && segment.end > start) {
      startNode = segment.node;
      startOffset = Math.max(0, start - segment.start);
    }
    if (segment.end >= end) {
      endNode = segment.node;
      endOffset = Math.max(0, end - segment.start);
      break;
    }
  }

  if (!startNode || !endNode) return null;

  try {
    const range = document.createRange();
    range.setStart(startNode, startOffset);
    range.setEnd(endNode, endOffset);
    return range;
  } catch {
    return null;
  }
}

function scrollRangeIntoView(root: HTMLElement, range: Range) {
  const rect = range.getBoundingClientRect();
  const containerRect = root.getBoundingClientRect();
  if (!rect.height && !rect.width) return;

  if (rect.top < containerRect.top + 24) {
    root.scrollTop -= containerRect.top - rect.top + 32;
  } else if (rect.bottom > containerRect.bottom - 24) {
    root.scrollTop += rect.bottom - containerRect.bottom + 32;
  }
}

function paintPreviewHighlights(
  root: HTMLElement,
  matches: FindMatch[],
  activeIndex: number,
) {
  if (typeof CSS === "undefined" || !("highlights" in CSS)) return;

  clearPreviewFindHighlights();
  if (matches.length === 0) return;

  const allRanges: Range[] = [];
  for (const match of matches) {
    const range = rangeForMatch(root, match.start, match.end);
    if (range) allRanges.push(range);
  }

  if (allRanges.length > 0) {
    CSS.highlights.set(FIND_HIGHLIGHT_ALL, new Highlight(...allRanges));
  }

  const safeIndex =
    ((activeIndex % matches.length) + matches.length) % matches.length;
  const activeMatch = matches[safeIndex];
  if (!activeMatch) return;

  const activeRange = rangeForMatch(root, activeMatch.start, activeMatch.end);
  if (activeRange) {
    CSS.highlights.set(FIND_HIGHLIGHT_ACTIVE, new Highlight(activeRange));
  }
}

export function scrollPreviewToFindMatch(
  root: HTMLElement,
  query: string,
  activeIndex: number,
  matchesInput?: FindMatch[],
  options: FindMatchOptions = {},
): number {
  clearFindHighlights(root);
  if (!query.trim()) return 0;

  const plainText = renderedPlainText(root);
  const matches = matchesInput ?? findMatches(plainText, query, options);
  if (matches.length === 0) return 0;

  paintPreviewHighlights(root, matches, activeIndex);

  const safeIndex =
    ((activeIndex % matches.length) + matches.length) % matches.length;
  const match = matches[safeIndex]!;
  const range = rangeForMatch(root, match.start, match.end);
  if (!range) return matches.length;

  scrollRangeIntoView(root, range);
  return matches.length;
}

/** @deprecated Use scrollPreviewToFindMatch */
export function highlightActiveFindMatch(
  root: HTMLElement,
  query: string,
  activeIndex: number,
): number {
  return scrollPreviewToFindMatch(root, query, activeIndex);
}
