import type { JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { createLiveExtensions } from "./liveExtensions";
import { restoreWikilinkMarkdown } from "./liveWikilink";

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

function isEmbed(node: JSONContent): boolean {
  return node.type === "embedBlock";
}

function serializeProseNodes(nodes: JSONContent[]): string {
  if (nodes.length === 0) return "";
  const md = getManager()
    .serialize({ type: "doc", content: nodes })
    .replace(/\n+$/, "");
  return restoreWikilinkMarkdown(md);
}

/**
 * TipTap JSON → markdown body (no frontmatter).
 * Fence/embed atoms dump stable source; prose uses TipTap markdown serialize.
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
    if (isEmbed(node)) {
      flushProse();
      const path = String(node.attrs?.path ?? "").trim();
      if (path) parts.push(`![[${path}]]`);
      continue;
    }
    proseBuf.push(node);
  }
  flushProse();

  return parts.join("\n\n").replace(/\n+$/, "") + (parts.length ? "\n" : "");
}
