import { wikilinkLabel } from "$lib/utils/formatVault";
import {
  colorSpanHtml,
  isMarkdownColorId,
  isMarkdownHexColor,
  normalizeMarkdownHexColor,
} from "$lib/utils/vaultMarkdownColors";

import {
  calloutDefaultTitle,
  calloutIconForTone,
  calloutIconSvg,
} from "$lib/styles/calloutIcons";

import { escapeAttr, escapeHtml } from "./escape";
import {
  buildFootnotesSectionHtml,
  planFootnotes,
  replaceFootnoteMarkers,
} from "./footnotes";
import { parseImageSizeToken } from "./imageSize";

const CALLOUT_LINE = /^>\s*\[!(\w+)\]\s*(.*)$/i;
const CALLOUT_CONT = /^>\s?(.*)$/;
const WIKI_IMAGE_EXT =
  /\.(png|jpe?g|gif|webp|svg|bmp|ico|heic|heif|avif)$/i;

/**
 * Obsidian image embeds `![[shot.png|400]]` → markdown images with size on the href.
 * Note embeds (`![[Other note]]`) are left for transclusion resolution.
 */
export function preprocessWikiImageEmbeds(source: string): string {
  return source.replace(
    /!\[\[([^\]|#]+)(?:\|([^\]]+))?\]\]/g,
    (match, target: string, pipe: string | undefined) => {
      const path = target.trim();
      if (!WIKI_IMAGE_EXT.test(path.split("?")[0]?.split("#")[0] ?? path)) {
        return match;
      }
      const token = pipe?.trim();
      if (token && parseImageSizeToken(token)) {
        return `![](${path}|${token})`;
      }
      if (token) {
        return `![${token}](${path})`;
      }
      return `![](${path})`;
    },
  );
}

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
    const title = match[2]?.trim() || calloutDefaultTitle(kind);
    index += 1;
    const body: string[] = [];

    while (index < lines.length) {
      const cont = lines[index].match(CALLOUT_CONT);
      if (!cont) break;
      body.push(cont[1] ?? "");
      index += 1;
    }

    const iconHtml = calloutIconSvg(calloutIconForTone(kind));
    const headerHtml = `<div class="markdown-callout-header"><span class="markdown-callout-icon">${iconHtml}</span><p class="markdown-callout-title">${escapeHtml(title)}</p></div>`;
    const bodyHtml = body
      .filter((line) => line.trim().length > 0)
      .map((line) => `<p>${escapeHtml(line)}</p>`)
      .join("");

    out.push(
      `<aside class="markdown-callout markdown-callout-${escapeAttr(kind)}" data-callout="${escapeAttr(kind)}">${headerHtml}<div class="markdown-callout-body">${bodyHtml}</div></aside>`,
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
  /\{\{(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|red|orange|yellow|green|blue|purple|pink)\|([\s\S]*?)\}\}/gi;
const LEGACY_COLOR_SPAN =
  /<span class="markdown-color markdown-color-(red|orange|yellow|green|blue|purple|pink)">([\s\S]*?)<\/span>/gi;

function replaceColorMarkup(line: string): string {
  let next = line.replace(COLOR_TAG, (_match, color: string, text: string) => {
    const token = color.trim();
    if (isMarkdownColorId(token)) {
      return colorSpanHtml(token.toLowerCase(), escapeHtml(text));
    }
    if (isMarkdownHexColor(token)) {
      const hex = normalizeMarkdownHexColor(token);
      return hex ? colorSpanHtml(hex, escapeHtml(text)) : _match;
    }
    return _match;
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

/** Strip `{{Label key:value}}` table headers down to labels for preview HTML. */
export function preprocessLedgerColumnHeaders(source: string): string {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let inFence = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
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

    const headerCells = splitPipeRowForPreview(line);
    const separatorCells = splitPipeRowForPreview(lines[i + 1] ?? "");
    if (
      headerCells &&
      separatorCells &&
      separatorCells.every((cell) => /^:?-+:?$/.test(cell.replace(/\s/g, "")))
    ) {
      out.push(
        `| ${headerCells.map((cell) => columnDisplayLabel(cell)).join(" | ")} |`,
      );
      continue;
    }
    out.push(line);
  }

  return out.join("\n");
}

function splitPipeRowForPreview(line: string): string[] | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("|") || !trimmed.endsWith("|")) return null;
  return trimmed
    .slice(1, -1)
    .split("|")
    .map((cell) => cell.trim());
}

import { preprocessTableOfContents } from "./toc";
import { preprocessLiquidEmbeds } from "./liquidEmbeds";
import { columnDisplayLabel } from "$lib/utils/ledgerSheet";

/**
 * Obsidian footnotes → superscript refs + footer section (skipped inside fences).
 * Inline bodies and definition text are escaped; Preview can still wrap with marked.
 */
export function preprocessFootnotes(source: string): string {
  const plan = planFootnotes(source);
  if (plan.orderedIds.length === 0) return source;

  const withMarkers = replaceFootnoteMarkers(plan.bodyWithoutDefs, plan.numberById);
  const footer = buildFootnotesSectionHtml(plan, (md) => escapeHtml(md));
  return `${withMarkers.replace(/\s+$/, "")}${footer}`;
}

export function preprocessMarkdown(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  const normalized = source.replace(/\r\n/g, "\n");
  const withHighlights = preprocessHighlights(normalized);
  const withColors = preprocessColorSpans(withHighlights);
  const withLedgerHeaders = preprocessLedgerColumnHeaders(withColors);
  const withWikiImages = preprocessWikiImageEmbeds(withLedgerHeaders);
  const withWikilinks = preprocessWikilinks(withWikiImages, titleByPath);
  const withCallouts = preprocessCallouts(withWikilinks);
  const withFootnotes = preprocessFootnotes(withCallouts);
  const withToc = preprocessTableOfContents(withFootnotes);
  return preprocessLiquidEmbeds(withToc);
}
