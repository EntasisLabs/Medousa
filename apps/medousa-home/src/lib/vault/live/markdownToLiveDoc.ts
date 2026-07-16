import type { JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { splitMarkdownSegments } from "./fenceCard";
import { fenceAttrsFromRaw } from "./fenceBlockExtension";
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

function proseToNodes(text: string): JSONContent[] {
  const trimmed = text.replace(/^\n+/, "").replace(/\n+$/, "");
  if (!trimmed) return [];
  const doc = getManager().parse(trimmed);
  return doc.content ?? [];
}

/** Markdown body (no frontmatter) → TipTap JSON doc with fence atoms. */
export function markdownToLiveDoc(body: string): JSONContent {
  const segments = splitMarkdownSegments(body);
  const content: JSONContent[] = [];

  for (const seg of segments) {
    if (seg.kind === "prose") {
      content.push(...proseToNodes(seg.text));
      continue;
    }
    content.push({
      type: "fenceBlock",
      attrs: fenceAttrsFromRaw(seg.raw),
    });
  }

  if (content.length === 0) {
    content.push({ type: "paragraph" });
  }

  return { type: "doc", content };
}
