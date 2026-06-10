import {
  MARKDOWN_COLOR_CLOSE_TAG,
  markdownColorOpenTag,
  type MarkdownColorId,
} from "$lib/utils/vaultMarkdownColors";

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

export type { MarkdownColorId };

export type SlashBlockId =
  | "h1"
  | "h2"
  | "h3"
  | "bullet"
  | "numbered"
  | "checkbox"
  | "link"
  | "divider"
  | "quote";

export interface EditResult {
  content: string;
  selectionStart: number;
  selectionEnd: number;
}

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

function wrapSelection(
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
      return wrapSelection(content, selectionStart, selectionEnd, "**", "**");
    case "italic":
      return wrapSelection(content, selectionStart, selectionEnd, "*", "*");
    case "code":
      return wrapSelection(content, selectionStart, selectionEnd, "`", "`", "code");
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
      return wrapSelection(content, selectionStart, selectionEnd, "==", "==");
    default:
      return { content, selectionStart, selectionEnd };
  }
}

export function applyMarkdownColor(
  content: string,
  selectionStart: number,
  selectionEnd: number,
  color: MarkdownColorId,
): EditResult {
  return wrapSelection(
    content,
    selectionStart,
    selectionEnd,
    markdownColorOpenTag(color),
    MARKDOWN_COLOR_CLOSE_TAG,
    "text",
  );
}

export function insertSlashBlock(
  content: string,
  cursorIndex: number,
  block: SlashBlockId,
): EditResult {
  const lineStart = lineStartIndex(content, cursorIndex);
  const line = content.slice(lineStart, cursorIndex);
  const slashMatch = line.match(/^(\s*)(\/[\w-]*)$/);
  const replaceStart = slashMatch
    ? lineStart + (slashMatch[1]?.length ?? 0)
    : cursorIndex;

  const templates: Record<SlashBlockId, string> = {
    h1: "# ",
    h2: "## ",
    h3: "### ",
    bullet: "- ",
    numbered: "1. ",
    checkbox: "- [ ] ",
    link: "[label](url)",
    divider: "---\n",
    quote: "> ",
  };

  const insert = templates[block];
  const next = `${content.slice(0, replaceStart)}${insert}${content.slice(cursorIndex)}`;
  const cursor = replaceStart + insert.length;
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
