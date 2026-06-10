import { wikilinkLabel } from "$lib/utils/formatVault";
import {
  colorSpanHtml,
  isMarkdownColorId,
} from "$lib/utils/vaultMarkdownColors";

import { escapeAttr, escapeHtml } from "./escape";

const CALLOUT_LINE = /^>\s*\[!(\w+)\]\s*(.*)$/i;
const CALLOUT_CONT = /^>\s?(.*)$/;

/** Obsidian wikilinks → internal link protocol for the marked renderer. */
export function preprocessWikilinks(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  return source.replace(
    /\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]/g,
    (_match, target: string, heading: string | undefined, alias: string | undefined) => {
      const path = target.trim();
      const label = alias?.trim() || wikilinkLabel(path, titleByPath);
      const hash = heading?.trim() ? `#${heading.trim()}` : "";
      const href = `wikilink:${encodeURIComponent(path + hash)}`;
      return `[${label}](${href})`;
    },
  );
}

/** Obsidian callouts (`> [!note]`) → HTML blocks marked passes through. */
export function preprocessCallouts(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let index = 0;

  while (index < lines.length) {
    const match = lines[index].match(CALLOUT_LINE);
    if (!match) {
      out.push(lines[index]);
      index += 1;
      continue;
    }

    const kind = match[1].toLowerCase();
    const title = match[2]?.trim() ?? kind;
    index += 1;
    const body: string[] = [];

    while (index < lines.length) {
      const cont = lines[index].match(CALLOUT_CONT);
      if (!cont) break;
      body.push(cont[1] ?? "");
      index += 1;
    }

    const titleHtml = title
      ? `<p class="markdown-callout-title">${escapeHtml(title)}</p>`
      : "";
    const bodyHtml = body
      .filter((line) => line.trim().length > 0)
      .map((line) => `<p>${escapeHtml(line)}</p>`)
      .join("");

    out.push(
      `<div class="markdown-callout markdown-callout-${escapeAttr(kind)}" data-callout="${escapeAttr(kind)}">${titleHtml}<div class="markdown-callout-body">${bodyHtml}</div></div>`,
    );
    out.push("");
  }

  return out.join("\n");
}

/** Obsidian-style `==highlight==` (skipped inside fenced code blocks). */
export function preprocessHighlights(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let inFence = false;

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

    out.push(
      line.replace(/==([^=\n][^=\n]*?)==/g, '<mark class="markdown-highlight">$1</mark>'),
    );
  }

  return out.join("\n");
}

const COLOR_TAG =
  /\{\{(red|orange|yellow|green|blue|purple|pink)\|([\s\S]*?)\}\}/gi;
const LEGACY_COLOR_SPAN =
  /<span class="markdown-color markdown-color-(red|orange|yellow|green|blue|purple|pink)">([\s\S]*?)<\/span>/gi;

function replaceColorMarkup(line: string): string {
  let next = line.replace(COLOR_TAG, (_match, color: string, text: string) => {
    const id = color.toLowerCase();
    return isMarkdownColorId(id) ? colorSpanHtml(id, text) : _match;
  });
  next = next.replace(LEGACY_COLOR_SPAN, (_match, color: string, text: string) => {
    const id = color.toLowerCase();
    return isMarkdownColorId(id) ? colorSpanHtml(id, text) : _match;
  });
  return next;
}

/** `{{red|text}}` and legacy HTML color spans → styled spans for preview. */
export function preprocessColorSpans(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let inFence = false;

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

    out.push(replaceColorMarkup(line));
  }

  return out.join("\n");
}

export function preprocessMarkdown(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  const normalized = source.replace(/\r\n/g, "\n");
  const withHighlights = preprocessHighlights(normalized);
  const withColors = preprocessColorSpans(withHighlights);
  return preprocessCallouts(preprocessWikilinks(withColors, titleByPath));
}
