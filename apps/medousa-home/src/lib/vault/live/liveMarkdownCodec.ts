import {
  parseFrontmatterTags,
  serializeFrontmatter,
  stripFrontmatter,
} from "$lib/utils/vaultFrontmatter";
import type { JSONContent } from "@tiptap/core";
import { markdownToLiveDoc } from "./markdownToLiveDoc";
import { liveDocToMarkdown } from "./liveDocToMarkdown";

export type LiveMarkdownParts = {
  frontmatter: string | null;
  body: string;
  tags: string[];
  doc: JSONContent;
};

/** Full note markdown → Live doc + frontmatter metadata. */
export function parseLiveMarkdown(full: string): LiveMarkdownParts {
  const { content: body, frontmatter } = stripFrontmatter(full);
  return {
    frontmatter,
    body,
    tags: parseFrontmatterTags(frontmatter),
    doc: markdownToLiveDoc(body),
  };
}

/** Live doc + stored frontmatter → full note markdown. */
export function serializeLiveMarkdown(
  doc: JSONContent,
  frontmatter: string | null,
): string {
  const body = liveDocToMarkdown(doc);
  if (frontmatter == null) return body;
  return serializeFrontmatter(frontmatter, body);
}

/** Significant text for “did we eat content?” checks (fences + prose). */
export function significantLiveText(markdown: string): string {
  const { content } = stripFrontmatter(markdown);
  return content
    .replace(/```[\s\S]*?```/g, (m) => m)
    .replace(/\s+/g, " ")
    .trim();
}
