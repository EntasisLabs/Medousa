import {
  SLASH_BOARD_TEMPLATE,
  SLASH_TABLE_TEMPLATE,
  SLASH_TOC_TEMPLATE,
} from "$lib/utils/vaultTemplates";
import {
  LIQUID_CALLOUT_TEMPLATE,
  LIQUID_CARD_TEMPLATE,
  LIQUID_CHART_TEMPLATE,
  LIQUID_DASHBOARD_TEMPLATE,
  LIQUID_REPORT_TEMPLATE,
} from "$lib/utils/liquidFenceTemplates";
import {
  MARKDOWN_COLOR_IDS,
  colorMarkupToken,
  isMarkdownColorId,
  isMarkdownHexColor,
  normalizeMarkdownHexColor,
  type MarkdownColorId,
  type MarkdownColorToken,
} from "$lib/utils/vaultMarkdownColors";
import { ensureKanbanBoardFrontmatter } from "$lib/utils/markdownKanban";

export type MarkdownFormatAction =
  | "bold"
  | "italic"
  | "code"
  | "link"
  | "h1"
  | "h2"
  | "h3"
  | "bullet"
  | "numbered"
  | "checkbox"
  | "highlight";

export type { MarkdownColorId, MarkdownColorToken };

export type SlashBlockId =
  | "h1"
  | "h2"
  | "h3"
  | "bullet"
  | "numbered"
  | "checkbox"
  | "link"
  | "wikilink"
  | "divider"
  | "quote"
  | "callout"
  | "liquid_callout"
  | "liquid_card"
  | "liquid_chart"
  | "liquid_dashboard"
  | "liquid_report"
  | "embed"
  | "view"
  | "board"
  | "table"
  | "toc";

export interface EditResult {
  content: string;
  selectionStart: number;
  selectionEnd: number;
}

const COLOR_SYNTAX =
  /^\{\{(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|red|orange|yellow|green|blue|purple|pink)\|([\s\S]*)\}\}$/i;
const LEGACY_COLOR_SPAN =
  /^<span class="markdown-color markdown-color-(red|orange|yellow|green|blue|purple|pink)">([\s\S]*)<\/span>$/i;

function lineRangeAt(content: string, index: number): { start: number; end: number } {
  const start = content.lastIndexOf("\n", Math.max(0, index - 1)) + 1;
  const nextBreak = content.indexOf("\n", index);
  const end = nextBreak === -1 ? content.length : nextBreak;
  return { start, end };
}

function selectedLines(content: string, selectionStart: number, selectionEnd: number): string[] {
  const startLine = lineRangeAt(content, selectionStart).start;
  const endLine = lineRangeAt(content, Math.max(selectionStart, selectionEnd - 1)).end;
  return content.slice(startLine, endLine).split("\n");
}

function lineStartIndex(content: string, index: number): number {
  return lineRangeAt(content, index).start;
}

function replaceRange(
  content: string,
  start: number,
  end: number,
  replacement: string,
): EditResult {
  const next = `${content.slice(0, start)}${replacement}${content.slice(end)}`;
  return {
    content: next,
    selectionStart: start,
    selectionEnd: start + replacement.length,
  };
}

function unwrapHighlight(text: string): string | null {
  if (text.startsWith("==") && text.endsWith("==") && text.length >= 4) {
    return text.slice(2, -2);
  }
  return null;
}

function stripInlineMarkup(text: string): string {
  let current = text;
  for (let pass = 0; pass < 4; pass++) {
    const colored = unwrapColorMarkup(current);
    if (colored) {
      current = colored.inner;
      continue;
    }
    const highlighted = unwrapHighlight(current);
    if (highlighted != null) {
      current = highlighted;
      continue;
    }
    break;
  }
  return current;
}

function unwrapColorMarkup(text: string): { color: MarkdownColorToken; inner: string } | null {
  const syntax = text.match(COLOR_SYNTAX);
  if (syntax) {
    const token = syntax[1]!;
    if (isMarkdownColorId(token)) {
      return { color: token.toLowerCase() as MarkdownColorId, inner: syntax[2]! };
    }
    if (isMarkdownHexColor(token)) {
      const hex = normalizeMarkdownHexColor(token);
      if (hex) return { color: hex, inner: syntax[2]! };
    }
  }
  const legacy = text.match(LEGACY_COLOR_SPAN);
  if (legacy) {
    const color = legacy[1]!.toLowerCase();
    if (MARKDOWN_COLOR_IDS.includes(color as MarkdownColorId)) {
      return { color: color as MarkdownColorId, inner: legacy[2]! };
    }
  }
  return null;
}

function colorsMatch(a: MarkdownColorToken, b: MarkdownColorToken): boolean {
  if (isMarkdownColorId(a) && isMarkdownColorId(b)) {
    return a.toLowerCase() === b.toLowerCase();
  }
  const left = isMarkdownHexColor(a) ? normalizeMarkdownHexColor(a) : null;
  const right = isMarkdownHexColor(b) ? normalizeMarkdownHexColor(b) : null;
  if (left && right) return left === right;
  return false;
}

function toggleWrapSelection(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  before: string,
  after: string,
  placeholder = "text",
): EditResult {
  const hasSelection = selectionStart !== selectionEnd;
  const selected = hasSelection
    ? content.slice(selectionStart, selectionEnd)
    : placeholder;

  if (
    selected.startsWith(before) &&
    selected.endsWith(after) &&
    selected.length >= before.length + after.length
  ) {
    const inner = selected.slice(before.length, selected.length - after.length);
    return replaceRange(content, selectionStart, selectionEnd, inner);
  }

  return wrapSelection(content, selectionStart, selectionEnd, before, after, placeholder);
}

function wrapSelection(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  before: string,
  after: string,
  placeholder = "text",
): EditResult {
  const hasSelection = selectionStart !== selectionEnd;
  let selected = hasSelection ? content.slice(selectionStart, selectionEnd) : placeholder;

  const wrappedColor = unwrapColorMarkup(selected);
  if (wrappedColor) {
    selected = wrappedColor.inner;
  } else {
    selected = stripInlineMarkup(selected);
  }

  const next = `${content.slice(0, selectionStart)}${before}${selected}${after}${content.slice(selectionEnd)}`;
  const innerStart = selectionStart + before.length;
  const innerEnd = innerStart + selected.length;
  return {
    content: next,
    selectionStart: innerStart,
    selectionEnd: hasSelection ? innerEnd : innerEnd,
  };
}

function prefixLines(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  prefix: string,
  numbered = false,
): EditResult {
  const startLine = lineRangeAt(content, selectionStart).start;
  const endLine = lineRangeAt(content, Math.max(selectionStart, selectionEnd - 1)).end;
  const block = content.slice(startLine, endLine);
  const lines = block.split("\n");
  const nextLines = lines.map((line, index) => {
    const stripped = line.replace(/^\s*([#>*\-+]|\d+\.)\s*/, "").trimStart();
    const base = stripped.length > 0 ? stripped : "";
    if (numbered) {
      return `${prefix}${index + 1}. ${base}`.trimEnd();
    }
    return `${prefix}${base}`.trimEnd();
  });
  const nextBlock = nextLines.join("\n");
  const next = `${content.slice(0, startLine)}${nextBlock}${content.slice(endLine)}`;
  return {
    content: next,
    selectionStart: startLine,
    selectionEnd: startLine + nextBlock.length,
  };
}

function setHeadingLevel(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  level: 1 | 2 | 3,
): EditResult {
  const marker = `${"#".repeat(level)} `;
  const startLine = lineRangeAt(content, selectionStart).start;
  const endLine = lineRangeAt(content, Math.max(selectionStart, selectionEnd - 1)).end;
  const block = content.slice(startLine, endLine);
  const lines = block.split("\n").map((line) => {
    const body = line.replace(/^\s*#{1,6}\s*/, "");
    return `${marker}${body}`.trimEnd();
  });
  const nextBlock = lines.join("\n");
  const next = `${content.slice(0, startLine)}${nextBlock}${content.slice(endLine)}`;
  return {
    content: next,
    selectionStart: startLine,
    selectionEnd: startLine + nextBlock.length,
  };
}

export function applyMarkdownFormat(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  action: MarkdownFormatAction,
): EditResult {
  switch (action) {
    case "bold":
      return toggleWrapSelection(content, selectionStart, selectionEnd, "**", "**");
    case "italic":
      return toggleWrapSelection(content, selectionStart, selectionEnd, "*", "*");
    case "code":
      return toggleWrapSelection(content, selectionStart, selectionEnd, "`", "`", "code");
    case "link":
      return wrapSelection(content, selectionStart, selectionEnd, "[", "](url)", "label");
    case "h1":
      return setHeadingLevel(content, selectionStart, selectionEnd, 1);
    case "h2":
      return setHeadingLevel(content, selectionStart, selectionEnd, 2);
    case "h3":
      return setHeadingLevel(content, selectionStart, selectionEnd, 3);
    case "bullet":
      return prefixLines(content, selectionStart, selectionEnd, "- ");
    case "numbered":
      return prefixLines(content, selectionStart, selectionEnd, "", true);
    case "checkbox":
      return prefixLines(content, selectionStart, selectionEnd, "- [ ] ");
    case "highlight":
      return toggleWrapSelection(content, selectionStart, selectionEnd, "==", "==");
    default:
      return { content, selectionStart, selectionEnd };
  }
}

export function applyMarkdownColor(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  color: MarkdownColorToken,
): EditResult {
  const token = isMarkdownColorId(color)
    ? (color.toLowerCase() as MarkdownColorId)
    : normalizeMarkdownHexColor(color);
  if (!token) {
    return { content, selectionStart, selectionEnd };
  }

  const hasSelection = selectionStart !== selectionEnd;
  const selected = hasSelection
    ? content.slice(selectionStart, selectionEnd)
    : "text";

  const wrapped = unwrapColorMarkup(selected);
  if (wrapped) {
    if (colorsMatch(wrapped.color, token)) {
      return replaceRange(content, selectionStart, selectionEnd, wrapped.inner);
    }
    return replaceRange(
      content,
      selectionStart,
      selectionEnd,
      colorMarkupToken(token, stripInlineMarkup(wrapped.inner)),
    );
  }

  const inner = stripInlineMarkup(selected);
  return replaceRange(
    content,
    selectionStart,
    selectionEnd,
    colorMarkupToken(token, inner),
  );
}

export function insertSlashBlock(
  content: string,
  cursorIndex: number,
  block: SlashBlockId,
): EditResult {
  const templates: Partial<Record<SlashBlockId, string>> = {
    h1: "# ",
    h2: "## ",
    h3: "### ",
    bullet: "- ",
    numbered: "1. ",
    checkbox: "- [ ] ",
    link: "[label](url)",
    wikilink: "",
    divider: "---\n",
    quote: "> ",
    callout: "",
    liquid_callout: LIQUID_CALLOUT_TEMPLATE,
    liquid_card: LIQUID_CARD_TEMPLATE,
    liquid_chart: LIQUID_CHART_TEMPLATE,
    liquid_dashboard: LIQUID_DASHBOARD_TEMPLATE,
    liquid_report: LIQUID_REPORT_TEMPLATE,
    embed: "",
    view: "",
    board: SLASH_BOARD_TEMPLATE,
    table: SLASH_TABLE_TEMPLATE,
    toc: SLASH_TOC_TEMPLATE,
  };

  const insert = templates[block] ?? "";
  return replaceSlashWith(content, cursorIndex, insert, block === "board");
}

/** Replace an open `/token` (or insert at cursor) with markdown. */
export function replaceSlashWith(
  content: string,
  cursorIndex: number,
  insert: string,
  ensureBoardFrontmatter = false,
): EditResult {
  const lineStart = lineStartIndex(content, cursorIndex);
  const line = content.slice(lineStart, cursorIndex);
  const slashMatch = line.match(/^(\s*)(\/[\w-]*)$/);
  const replaceStart = slashMatch
    ? lineStart + (slashMatch[1]?.length ?? 0)
    : cursorIndex;

  const nextRaw = `${content.slice(0, replaceStart)}${insert}${content.slice(cursorIndex)}`;
  const next = ensureBoardFrontmatter
    ? ensureKanbanBoardFrontmatter(nextRaw)
    : nextRaw;
  const shift = next.length - nextRaw.length;
  const cursor = replaceStart + insert.length + shift;
  return {
    content: next,
    selectionStart: cursor,
    selectionEnd: cursor,
  };
}

export function serializeCalloutBlock(
  kind: string,
  title: string,
  body: string,
): string {
  const safeKind = kind.trim().toLowerCase() || "note";
  const heading = title.trim() || safeKind;
  const lines = [`> [!${safeKind}] ${heading}`];
  const bodyLines = body.replace(/\r\n/g, "\n").split("\n");
  if (bodyLines.every((line) => !line.trim())) {
    lines.push("> ");
  } else {
    for (const line of bodyLines) {
      lines.push(`> ${line}`);
    }
  }
  return `${lines.join("\n")}\n\n`;
}

export function serializeTransclusion(path: string): string {
  const token = path.replace(/\.md$/i, "");
  return `![[${token}]]\n\n`;
}

export function insertVaultWikilink(
  content: string,
  cursorIndex: number,
  path: string,
  label: string,
): EditResult {
  const lineStart = lineStartIndex(content, cursorIndex);
  const line = content.slice(lineStart, cursorIndex);
  const slashMatch = line.match(/^(\s*)(\/[\w-]*)$/);
  const replaceStart = slashMatch
    ? lineStart + (slashMatch[1]?.length ?? 0)
    : cursorIndex;

  const token = path.replace(/\.md$/i, "");
  const insert = `[[${token}|${label.trim() || token}]]`;
  const next = `${content.slice(0, replaceStart)}${insert}${content.slice(cursorIndex)}`;
  const cursor = replaceStart + insert.length;
  return {
    content: next,
    selectionStart: cursor,
    selectionEnd: cursor,
  };
}

export function slashMenuFilter(content: string, cursorIndex: number): string {
  const lineStart = lineStartIndex(content, cursorIndex);
  const line = content.slice(lineStart, cursorIndex);
  const match = line.match(/^\s*\/([\w-]*)$/);
  return match?.[1]?.toLowerCase() ?? "";
}

export function insertTextAtCursor(
  content: string,
  cursorIndex: number,
  insert: string,
): EditResult {
  const next = `${content.slice(0, cursorIndex)}${insert}${content.slice(cursorIndex)}`;
  const cursor = cursorIndex + insert.length;
  return {
    content: next,
    selectionStart: cursor,
    selectionEnd: cursor,
  };
}

export function shouldOpenSlashMenu(content: string, cursorIndex: number): boolean {
  const lineStart = lineStartIndex(content, cursorIndex);
  const linePrefix = content.slice(lineStart, cursorIndex);
  return /^\s*\/[\w-]*$/.test(linePrefix);
}

export function hasSelection(selectionStart: number, selectionEnd: number): boolean {
  return selectionStart !== selectionEnd;
}

export function selectedLineCount(
  content: string,
  selectionStart: number,
  selectionEnd: number,
): number {
  return selectedLines(content, selectionStart, selectionEnd).length;
}
