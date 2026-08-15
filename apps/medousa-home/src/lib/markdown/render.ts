import DOMPurify from "dompurify";
import { Marked, type Tokens } from "marked";

import { parseWikilinkTarget, resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import type { VaultNote } from "$lib/types/vault";
import { plainHeadingText, uniqueHeadingSlug } from "$lib/markdown/headingRender";
import { stripTrailingBlockIdHtml } from "$lib/markdown/blockAnchors";
import { escapeAttr, escapeHtml } from "./escape";
import { preprocessMarkdown } from "./preprocess";
import {
  imageSizeStyle,
  splitImageAltSize,
  splitImageHrefSize,
} from "./imageSize";
import { enhanceResumePresentation } from "./resumePresentation";
import { isLocalImageHref, isRemoteImageHref } from "$lib/utils/vaultLocalImages";

export interface MarkdownRenderOptions {
  titleByPath?: Map<string, string>;
  sourcePath?: string | null;
  knownPaths?: ReadonlySet<string>;
  interactiveTasks?: boolean;
  resolveLocalImages?: boolean;
}

function notesStubFromKnown(paths: ReadonlySet<string> | undefined): VaultNote[] {
  if (!paths) return [];
  return [...paths].map(
    (path) =>
      ({
        path,
        title: path.split("/").pop()?.replace(/\.md$/i, "") ?? path,
      }) as VaultNote,
  );
}

function wikilinkIsUnresolved(
  target: string,
  options: MarkdownRenderOptions,
): boolean {
  const known = options.knownPaths;
  if (!known || known.size === 0) return false;
  const { pathToken, heading } = parseWikilinkTarget(target);
  const token = pathToken.trim();
  // Same-note fragment links (`[[#Heading]]` / `[[#^id]]`) resolve to the open note.
  if (!token && heading) return false;
  if (!token) return true;
  return (
    resolveWikilinkTarget(
      token,
      options.sourcePath ?? null,
      notesStubFromKnown(known),
    ) === null
  );
}

function createMarked(
  options: MarkdownRenderOptions,
  headingSlugCounts: Map<string, number>,
  taskCheckboxIndex: { value: number },
): Marked {
  const parser = new Marked();
  parser.use({
    gfm: true,
    breaks: false,
  });

  parser.use({
    renderer: {
      heading(this: { parser: { parseInline: (t: Tokens.Heading["tokens"]) => string } }, token: Tokens.Heading) {
        const html = this.parser.parseInline(token.tokens);
        const stripped = stripTrailingBlockIdHtml(html);
        const plain = plainHeadingText(stripped.html);
        const slug = uniqueHeadingSlug(plain, headingSlugCounts);
        const blockAttr = stripped.blockId
          ? ` data-block-id="${escapeAttr(stripped.blockId)}"`
          : "";
        return `<h${token.depth} id="${escapeAttr(slug)}" class="markdown-heading" data-heading-slug="${escapeAttr(slug)}"${blockAttr}>${stripped.html}</h${token.depth}>`;
      },
      paragraph(this: { parser: { parseInline: (t: Tokens.Paragraph["tokens"]) => string } }, token: Tokens.Paragraph) {
        const html = this.parser.parseInline(token.tokens);
        const stripped = stripTrailingBlockIdHtml(html);
        const blockAttr = stripped.blockId
          ? ` id="^${escapeAttr(stripped.blockId)}" data-block-id="${escapeAttr(stripped.blockId)}"`
          : "";
        return `<p${blockAttr}>${stripped.html}</p>\n`;
      },
      link({ href, title, text }: Tokens.Link) {
        if (href?.startsWith("wikilink:")) {
          const target = decodeURIComponent(href.slice("wikilink:".length));
          const label = escapeHtml(text);
          const unresolved = wikilinkIsUnresolved(target, options);
          const className = unresolved
            ? "markdown-wikilink markdown-wikilink-unresolved"
            : "markdown-wikilink";
          const unresolvedAttr = unresolved ? ' data-wikilink-unresolved="true"' : "";
          return `<span class="${className}" role="link" tabindex="0" data-wikilink="${escapeAttr(target)}" title="${escapeAttr(target)}"${unresolvedAttr}>${label}</span>`;
        }
        const safeHref = escapeAttr(href ?? "#");
        const titleAttr = title ? ` title="${escapeAttr(title)}"` : "";
        const guideLink = href?.startsWith("guide:");
        if (guideLink) {
          return `<a href="${safeHref}" class="markdown-guide-link" data-guide-href="${safeHref}"${titleAttr}>${escapeHtml(text)}</a>`;
        }
        const external =
          href?.startsWith("http://") || href?.startsWith("https://");
        const internalHash = href?.startsWith("#");
        const linkLooksLikeUrl =
          external &&
          (text.trim() === href?.trim() ||
            text.trim().startsWith("http") ||
            (href?.length ?? 0) > 48);
        const classParts = [];
        if (linkLooksLikeUrl) classParts.push("markdown-external-link");
        if (internalHash) classParts.push("markdown-heading-link");
        const classAttr = classParts.length
          ? ` class="${classParts.join(" ")}"`
          : "";
        const targetAttr = internalHash ? "" : ' target="_blank" rel="noopener noreferrer"';
        return `<a href="${safeHref}"${classAttr}${titleAttr}${targetAttr}>${escapeHtml(text)}</a>`;
      },
      code({ text, lang }: Tokens.Code) {
        const language = (lang ?? "").trim();
        if (language === "mermaid") {
          const safe = text.replace(/<\/pre/gi, "");
          return `<pre class="mermaid">${safe}</pre>`;
        }
        if (language.toLowerCase() === "draw") {
          return `<div class="medousa-draw-embed liquid-md-embed" data-draw-embed=""><pre class="medousa-draw-source">${escapeHtml(text)}</pre></div>`;
        }
        const langLabel = language
          ? `<span class="markdown-code-lang">${escapeHtml(language)}</span>`
          : `<span class="markdown-code-lang markdown-code-lang-muted">code</span>`;
        const className = language
          ? `language-${escapeHtml(language)}`
          : "language-text";
        return `<div class="markdown-code-block"><div class="markdown-code-header">${langLabel}</div><pre class="markdown-pre"><code class="markdown-code ${className}">${escapeHtml(text)}</code></pre></div>`;
      },
      checkbox({ checked }: Tokens.Checkbox) {
        if (!options.interactiveTasks) {
          return `<input ${checked ? 'checked="" ' : ""}disabled="" type="checkbox"> `;
        }
        const index = taskCheckboxIndex.value;
        taskCheckboxIndex.value += 1;
        return `<input ${checked ? 'checked="" ' : ""}type="checkbox" class="vault-preview-task" data-vault-task="${index}" aria-label="Toggle task"> `;
      },
      image({ href, title, text }: Tokens.Image) {
        const { href: cleanHref, size: hrefSize } = splitImageHrefSize(
          href ?? "",
        );
        const { alt: cleanAlt, size: altSize } = splitImageAltSize(text || "");
        const size = hrefSize ?? altSize;
        const alt = escapeHtml(cleanAlt || title || "");
        const titleAttr = title ? ` title="${escapeAttr(title)}"` : "";
        const sizeAttr = size
          ? ` style="${escapeAttr(imageSizeStyle(size))}"`
          : "";
        const sizeClass = size ? " markdown-image--sized" : "";
        if (
          options.resolveLocalImages &&
          cleanHref &&
          !isRemoteImageHref(cleanHref) &&
          isLocalImageHref(cleanHref)
        ) {
          return `<figure class="markdown-image markdown-image-local${sizeClass}"><img class="markdown-local-image" data-local-image="${escapeAttr(cleanHref)}" alt="${alt}"${titleAttr}${sizeAttr} loading="lazy" decoding="async"></figure>`;
        }
        const safeHref = escapeAttr(cleanHref);
        return `<figure class="markdown-image${sizeClass}"><img src="${safeHref}" alt="${alt}"${titleAttr}${sizeAttr} loading="lazy" decoding="async"></figure>`;
      },
    },
  });
  return parser;
}

function sanitizeHtml(html: string): string {
  if (typeof window === "undefined") {
    return html;
  }
  return DOMPurify.sanitize(html, {
    ADD_ATTR: [
      "target",
      "rel",
      "data-callout",
      "data-wikilink",
      "data-wikilink-unresolved",
      "data-heading-slug",
      "data-heading-link",
      "data-guide-href",
      "data-open-vault-note",
      "data-view-source",
      "id",
      "role",
      "tabindex",
      "class",
      "style",
      "type",
      "disabled",
      "checked",
      "aria-label",
      "data-vault-task",
      "data-transclude-path",
      "data-transclude-heading",
      "data-local-image",
      "data-view-csv",
      "data-copy-view-csv",
      "data-edit-view-index",
      "data-edit-chart-index",
      "data-liquid-embed",
      "data-liquid-props",
      "data-liquid-icon",
      "data-draw-embed",
      "data-footnote-ref",
      "data-block-id",
      "data-md-font",
      "data-md-size",
      "src",
      "loading",
      "decoding",
      "value",
    ],
    ADD_TAGS: [
      "input",
      "mark",
      "span",
      "nav",
      "aside",
      "header",
      "button",
      "figure",
      "figcaption",
      "sup",
      "section",
    ],
    // Keep Operator's Guide deep-links (`guide:chat`, `guide:chat#panes`).
    ALLOWED_URI_REGEXP:
      /^(?:(?:(?:f|ht)tps?|mailto|tel|callto|sms|cid|xmpp|guide):|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i,
  });
}

function normalizeRenderOptions(
  options?: Map<string, string> | MarkdownRenderOptions,
): MarkdownRenderOptions {
  if (options instanceof Map) {
    return { titleByPath: options };
  }
  if (options) return options;
  return {};
}

function renderMarkdownWithState(
  source: string,
  options: MarkdownRenderOptions,
  headingSlugCounts: Map<string, number>,
  taskCheckboxIndex: { value: number },
): string {
  if (!source.trim()) return "";
  const parser = createMarked(options, headingSlugCounts, taskCheckboxIndex);
  const preprocessed = preprocessMarkdown(source, options.titleByPath);
  const raw = parser.parse(preprocessed, { async: false }) as string;
  return sanitizeHtml(enhanceResumePresentation(wrapMarkdownTables(raw)));
}

export interface MarkdownRenderSession {
  /** Render a block once and advance heading/task identity for later blocks. */
  renderStable(source: string): string;
  /** Render a replaceable tail without advancing stable identity. */
  renderTail(source: string): string;
}

/** Invocation-scoped renderer state for one stable-block Markdown surface. */
export function createMarkdownRenderSession(
  options?: Map<string, string> | MarkdownRenderOptions,
): MarkdownRenderSession {
  const renderOptions = normalizeRenderOptions(options);
  const headingSlugCounts = new Map<string, number>();
  const taskCheckboxIndex = { value: 0 };
  return {
    renderStable(source) {
      return renderMarkdownWithState(
        source,
        renderOptions,
        headingSlugCounts,
        taskCheckboxIndex,
      );
    },
    renderTail(source) {
      return renderMarkdownWithState(
        source,
        renderOptions,
        new Map(headingSlugCounts),
        { value: taskCheckboxIndex.value },
      );
    },
  };
}

/**
 * Inline-only markdown for titles / headings (`**bold**`, *italics*, `code`,
 * links). Does not produce block wrappers — safe inside `<h*>` / spans.
 */
export function renderInlineMarkdown(source: string): string {
  if (!source.trim()) return "";
  const parser = createMarked({}, new Map(), { value: 0 });
  const raw = parser.parseInline(source, { async: false }) as string;
  return sanitizeHtml(raw);
}

/** Shared Obsidian-flavored markdown renderer for chat, vault, and journal preview. */
export function renderMarkdown(
  source: string,
  options?: Map<string, string> | MarkdownRenderOptions,
): string {
  return createMarkdownRenderSession(options).renderStable(source);
}

/** Strip tags for short-cell heuristics (skills / expertise matrices). */
function plainTableCellText(cellHtml: string): string {
  return cellHtml
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * Wide GFM tables with short cells → skills matrix chrome.
 * Narrative / wide-text tables keep the default data-card shell.
 */
export function markdownTableShellClass(tableHtml: string): string {
  const firstRow = tableHtml.match(/<tr\b[\s\S]*?<\/tr>/i)?.[0] ?? "";
  const headerCells =
    firstRow.match(/<t[hd]\b[^>]*>[\s\S]*?<\/t[hd]>/gi) ?? [];
  if (headerCells.length < 3) return "markdown-table-scroll";

  const cells = tableHtml.match(/<t[hd]\b[^>]*>[\s\S]*?<\/t[hd]>/gi) ?? [];
  if (cells.length === 0) return "markdown-table-scroll";

  const texts = cells.map(plainTableCellText);
  const maxLen = Math.max(...texts.map((t) => t.length), 0);
  const avgLen =
    texts.reduce((sum, t) => sum + t.length, 0) / Math.max(texts.length, 1);
  if (maxLen <= 80 && avgLen <= 42) {
    return "markdown-table-scroll markdown-table--matrix";
  }
  return "markdown-table-scroll";
}

/** Scroll shell around tables so overflow never forces `display:block` on `<table>`. */
function wrapMarkdownTables(html: string): string {
  return html.replace(/<table\b[^>]*>[\s\S]*?<\/table>/gi, (table) => {
    const shell = markdownTableShellClass(table);
    return `<div class="${shell}">${table}</div>`;
  });
}

/** Back-compat alias used by vault editor and legacy imports. */
export function renderMarkdownPreview(
  source: string,
  options?: Map<string, string> | MarkdownRenderOptions,
): string {
  return renderMarkdown(source, options);
}
