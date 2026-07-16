import type { JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { createLiveExtensions } from "./liveExtensions";

let manager: MarkdownManager | null = null;

function getManager(): MarkdownManager {
  if (!manager) {
    manager = new MarkdownManager({
      extensions: createLiveExtensions(),
      indentation: { style: "space", size: 2 },
    });
  }
  return manager;
}

function isFence(node: JSONContent): boolean {
  return node.type === "fenceBlock";
}

/** TipTap escapes `[[wikilinks]]`; restore vault link syntax. */
export function unescapeVaultWikilinks(md: string): string {
  return md
    .replace(/!\\\[\\\[(.+?)\\\]\\\]/g, "![[$1]]")
    .replace(/\\\[\\\[(.+?)\\\]\\\]/g, "[[$1]]");
}

function serializeProseNodes(nodes: JSONContent[]): string {
  if (nodes.length === 0) return "";
  const md = getManager().serialize({ type: "doc", content: nodes }).replace(/\n+$/, "");
  return unescapeVaultWikilinks(md);
}

/**
 * TipTap JSON → markdown body (no frontmatter).
 * Fence atoms dump `attrs.raw` byte-stable; prose uses TipTap markdown serialize.
 */
export function liveDocToMarkdown(doc: JSONContent): string {
  const children = doc.content ?? [];
  const parts: string[] = [];
  let proseBuf: JSONContent[] = [];

  const flushProse = () => {
    if (proseBuf.length === 0) return;
    const md = serializeProseNodes(proseBuf);
    if (md) parts.push(md);
    proseBuf = [];
  };

  for (const node of children) {
    if (isFence(node)) {
      flushProse();
      const raw = String(node.attrs?.raw ?? "").replace(/\s+$/, "");
      if (raw) parts.push(raw);
      continue;
    }
    proseBuf.push(node);
  }
  flushProse();

  return parts.join("\n\n").replace(/\n+$/, "") + (parts.length ? "\n" : "");
}
