import { preprocessPortableLiquidEmbeds } from "@medousa/liquid-markdown";

/** Prepare assistant Markdown for Obsidian's renderer and the shared DOM hydrator. */
export function prepareObsidianLiquidMarkdown(markdown: string): string {
  return preprocessPortableLiquidEmbeds(markdown);
}

/** Keep web/app URLs intact and normalize vault-link-style media paths. */
export function normalizeLiquidMediaSource(source: string): string {
  const trimmed = source.trim();
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed)) return trimmed;
  return trimmed.replace(/^<(.+)>$/, "$1");
}
