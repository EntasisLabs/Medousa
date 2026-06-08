import { wikilinkLabel } from "$lib/utils/formatVault";

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

export function preprocessMarkdown(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  const normalized = source.replace(/\r\n/g, "\n");
  return preprocessCallouts(preprocessWikilinks(normalized, titleByPath));
}
