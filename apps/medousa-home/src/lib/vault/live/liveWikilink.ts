import { preprocessWikilinks } from "$lib/markdown/preprocess";
import { wikilinkLabel } from "$lib/utils/formatVault";

/**
 * Convert TipTap/marked `[label](wikilink:…)` back to Obsidian `[[target]]` / `[[target|alias]]`.
 */
export function restoreWikilinkMarkdown(md: string): string {
  return md.replace(
    /\[([^\]]*)\]\(wikilink:([^)]+)\)/g,
    (_m, label: string, encoded: string) => {
      let target = encoded;
      try {
        target = decodeURIComponent(encoded);
      } catch {
        // keep raw
      }
      const defaultLabel = wikilinkLabel(target.split("#")[0] ?? target);
      if (!label || label === defaultLabel || label === target) {
        return `[[${target}]]`;
      }
      return `[[${target}|${label}]]`;
    },
  );
}

/** Prose markdown → wikilink hrefs TipTap Link can hold. */
export function proseMarkdownForLive(text: string): string {
  return preprocessWikilinks(text);
}

/** Split prose into prose chunks and `![[embed]]` atoms. */
export type ProseOrEmbed =
  | { kind: "prose"; text: string }
  | { kind: "embed"; path: string; raw: string };

export function splitProseEmbeds(text: string): ProseOrEmbed[] {
  const parts: ProseOrEmbed[] = [];
  const re = /!\[\[([^\]]+)\]\]/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    if (match.index > last) {
      parts.push({ kind: "prose", text: text.slice(last, match.index) });
    }
    parts.push({
      kind: "embed",
      path: (match[1] ?? "").trim(),
      raw: match[0],
    });
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    parts.push({ kind: "prose", text: text.slice(last) });
  }
  if (parts.length === 0) {
    parts.push({ kind: "prose", text });
  }
  return parts;
}
