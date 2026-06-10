import DOMPurify from "dompurify";
import { marked, type Tokens } from "marked";

import { escapeAttr, escapeHtml } from "./escape";
import { preprocessMarkdown } from "./preprocess";

let configured = false;

function configureMarked(): void {
  if (configured) return;
  configured = true;

  marked.use({
    gfm: true,
    breaks: false,
  });

  marked.use({
    renderer: {
      link({ href, title, text }: Tokens.Link) {
        if (href?.startsWith("wikilink:")) {
          const target = decodeURIComponent(href.slice("wikilink:".length));
          const label = escapeHtml(text);
          return `<span class="markdown-wikilink" data-wikilink="${escapeAttr(target)}" title="${escapeAttr(target)}">${label}</span>`;
        }
        const safeHref = escapeAttr(href ?? "#");
        const titleAttr = title ? ` title="${escapeAttr(title)}"` : "";
        return `<a href="${safeHref}"${titleAttr} target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
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
      "class",
      "style",
      "type",
      "disabled",
      "checked",
    ],
    ADD_TAGS: ["input", "mark", "span"],
  });
}

/** Shared Obsidian-flavored markdown renderer for chat, vault, and journal preview. */
export function renderMarkdown(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  if (!source.trim()) return "";

  configureMarked();
  const preprocessed = preprocessMarkdown(source, titleByPath);
  const raw = marked.parse(preprocessed, { async: false }) as string;
  return sanitizeHtml(raw);
}

/** Back-compat alias used by vault editor and legacy imports. */
export function renderMarkdownPreview(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  return renderMarkdown(source, titleByPath);
}
