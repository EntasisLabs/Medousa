import DOMPurify from "dompurify";
import { marked, type Tokens } from "marked";

import { parseWikilinkTarget, resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import type { VaultNote } from "$lib/types/vault";
import { plainHeadingText, uniqueHeadingSlug } from "$lib/markdown/headingRender";
import { escapeAttr, escapeHtml } from "./escape";
import { preprocessMarkdown } from "./preprocess";

export interface MarkdownRenderOptions {
  titleByPath?: Map<string, string>;
  sourcePath?: string | null;
  knownPaths?: ReadonlySet<string>;
  interactiveTasks?: boolean;
}

let previewTaskCheckboxIndex = 0;

let configured = false;
let activeRenderOptions: MarkdownRenderOptions = {};
let activeHeadingSlugCounts = new Map<string, number>();

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

function wikilinkIsUnresolved(target: string): boolean {
  const known = activeRenderOptions.knownPaths;
  if (!known || known.size === 0) return false;
  const { pathToken } = parseWikilinkTarget(target);
  const token = pathToken.trim();
  if (!token) return true;
  return (
    resolveWikilinkTarget(
      token,
      activeRenderOptions.sourcePath ?? null,
      notesStubFromKnown(known),
    ) === null
  );
}

function configureMarked(): void {
  if (configured) return;
  configured = true;

  marked.use({
    gfm: true,
    breaks: false,
  });

  marked.use({
    renderer: {
      heading({ text, depth }: Tokens.Heading) {
        const plain = plainHeadingText(text);
        const slug = uniqueHeadingSlug(plain, activeHeadingSlugCounts);
        return `<h${depth} id="${escapeAttr(slug)}" class="markdown-heading" data-heading-slug="${escapeAttr(slug)}">${text}</h${depth}>`;
      },
      link({ href, title, text }: Tokens.Link) {
        if (href?.startsWith("wikilink:")) {
          const target = decodeURIComponent(href.slice("wikilink:".length));
          const label = escapeHtml(text);
          const unresolved = wikilinkIsUnresolved(target);
          const className = unresolved
            ? "markdown-wikilink markdown-wikilink-unresolved"
            : "markdown-wikilink";
          const unresolvedAttr = unresolved ? ' data-wikilink-unresolved="true"' : "";
          return `<span class="${className}" role="link" tabindex="0" data-wikilink="${escapeAttr(target)}" title="${escapeAttr(target)}"${unresolvedAttr}>${label}</span>`;
        }
        const safeHref = escapeAttr(href ?? "#");
        const titleAttr = title ? ` title="${escapeAttr(title)}"` : "";
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
        const langLabel = language
          ? `<span class="markdown-code-lang">${escapeHtml(language)}</span>`
          : `<span class="markdown-code-lang markdown-code-lang-muted">code</span>`;
        const className = language
          ? `language-${escapeHtml(language)}`
          : "language-text";
        return `<div class="markdown-code-block"><div class="markdown-code-header">${langLabel}</div><pre class="markdown-pre"><code class="markdown-code ${className}">${escapeHtml(text)}</code></pre></div>`;
      },
      checkbox({ checked }: Tokens.Checkbox) {
        if (!activeRenderOptions.interactiveTasks) {
          return `<input ${checked ? 'checked="" ' : ""}disabled="" type="checkbox"> `;
        }
        const index = previewTaskCheckboxIndex;
        previewTaskCheckboxIndex += 1;
        return `<input ${checked ? 'checked="" ' : ""}type="checkbox" class="vault-preview-task" data-vault-task="${index}" aria-label="Toggle task"> `;
      },
    },
  });
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
    ],
    ADD_TAGS: ["input", "mark", "span", "nav", "aside", "header", "button"],
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

/** Shared Obsidian-flavored markdown renderer for chat, vault, and journal preview. */
export function renderMarkdown(
  source: string,
  options?: Map<string, string> | MarkdownRenderOptions,
): string {
  if (!source.trim()) return "";

  activeRenderOptions = normalizeRenderOptions(options);
  activeHeadingSlugCounts = new Map();
  previewTaskCheckboxIndex = 0;

  configureMarked();
  const preprocessed = preprocessMarkdown(
    source,
    activeRenderOptions.titleByPath,
  );
  const raw = marked.parse(preprocessed, { async: false }) as string;
  return sanitizeHtml(raw);
}

/** Back-compat alias used by vault editor and legacy imports. */
export function renderMarkdownPreview(
  source: string,
  options?: Map<string, string> | MarkdownRenderOptions,
): string {
  return renderMarkdown(source, options);
}
