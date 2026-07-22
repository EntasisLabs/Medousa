import type { JSONContent } from "@tiptap/core";
import { MarkdownManager } from "@tiptap/markdown";
import { preprocessTurboBlocks } from "$lib/markdown/preprocess";
import { splitMarkdownSegments } from "./fenceCard";
import { fenceAttrsFromRaw } from "./fenceBlockExtension";
import { embedAttrsFromPath } from "./embedBlockExtension";
import { createLiveExtensions } from "./liveExtensions";
import { proseMarkdownForLive, splitProseEmbeds } from "./liveWikilink";

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
  const prepared = proseMarkdownForLive(trimmed);
  const doc = getManager().parse(prepared);
  return doc.content ?? [];
}

/** Markdown body (no frontmatter) → TipTap JSON doc with fence + embed atoms. */
export function markdownToLiveDoc(body: string): JSONContent {
  const segments = splitMarkdownSegments(preprocessTurboBlocks(body));
  const content: JSONContent[] = [];

  for (const seg of segments) {
    if (seg.kind === "fence") {
      content.push({
        type: "fenceBlock",
        attrs: fenceAttrsFromRaw(seg.raw),
      });
      continue;
    }

    for (const part of splitProseEmbeds(seg.text)) {
      if (part.kind === "embed") {
        content.push({
          type: "embedBlock",
          attrs: embedAttrsFromPath(part.path),
        });
        continue;
      }
      content.push(...proseToNodes(part.text));
    }
  }

  if (content.length === 0) {
    content.push({ type: "paragraph" });
  }

  return { type: "doc", content };
}
