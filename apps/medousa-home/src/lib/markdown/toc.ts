import { escapeAttr, escapeHtml } from "./escape";
import { extractMarkdownHeadings } from "$lib/utils/headingSlug";

const TOC_BLOCK = /```medousa-toc\s*\n?```/gi;

/** Replace ` ```medousa-toc``` ` with an in-doc table of contents. */
export function preprocessTableOfContents(source: string): string {
  if (!/```medousa-toc/i.test(source)) {
    return source;
  }

  const headings = extractMarkdownHeadings(source);
  if (headings.length === 0) {
    return source.replace(TOC_BLOCK, "");
  }

  const items = headings
    .map((heading) => {
      const indent = Math.max(0, heading.depth - 1);
      return `<li class="markdown-toc-item markdown-toc-depth-${heading.depth}" style="margin-left:${indent * 0.75}rem"><a href="#${escapeAttr(heading.slug)}" class="markdown-toc-link" data-heading-link="${escapeAttr(heading.slug)}">${escapeHtml(heading.text)}</a></li>`;
    })
    .join("");

  const nav = `<nav class="markdown-toc" aria-label="Table of contents"><p class="markdown-toc-title">Contents</p><ul class="markdown-toc-list">${items}</ul></nav>`;

  return source.replace(TOC_BLOCK, nav);
}
