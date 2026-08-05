import { preprocessLiquidEmbeds } from "@medousa/liquid-markdown";
import {
  hydrateLiquidMarkdown,
  type LiquidBrowserHydrateOptions,
  type LiquidHydrationHandle,
} from "@medousa/liquid-markdown/browser";

export interface MedousaLiquidWebviewApi {
  renderMarkdown(markdown: string): string;
  hydrate(root: HTMLElement, options?: LiquidBrowserHydrateOptions): LiquidHydrationHandle;
}

type TrustedFragment = { token: string; html: string; block: boolean };

const CHART_SHELL_PATTERN = /<div class="liquid-chart-shell" data-edit-chart-index="\d+"><div class="liquid-chart-toolbar"><button type="button" class="liquid-chart-configure">Configure<\/button><\/div>(<div class="liquid-md-embed" data-liquid-embed="chart" data-liquid-props="[A-Za-z0-9+/=]+"><\/div>)<\/div>/g;
const EMBED_PATTERN = /<div class="liquid-md-embed" data-liquid-embed="[a-z_]+" data-liquid-props="[A-Za-z0-9+/=]+"><\/div>/g;
const ICON_PATTERN = /<span class="liquid-md-icon" data-liquid-icon="[a-z0-9-]+" aria-hidden="true"><\/span>/g;
const KANBAN_PATTERN = /<div class="liquid-mini-kanban" data-liquid-static="kanban"><p class="liquid-mini-kanban__label">Board<\/p><div class="liquid-mini-kanban__board">(?:<div class="liquid-mini-kanban__column"><p class="liquid-mini-kanban__column-title">[^<>]*<\/p><div class="liquid-mini-kanban__cards">(?:<div class="liquid-mini-kanban__card">[^<>]*<\/div>)*<\/div><\/div>)*<\/div><\/div>/g;

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    if (character === "&") return "&amp;";
    if (character === "<") return "&lt;";
    if (character === ">") return "&gt;";
    if (character === '"') return "&quot;";
    return "&#39;";
  });
}

function safeUrl(value: string): string {
  try {
    const url = new URL(value);
    return ["http:", "https:", "medousa:"].includes(url.protocol) ? value : "";
  } catch {
    return "";
  }
}

function extractTrustedLiquid(markdown: string): {
  source: string;
  fragments: TrustedFragment[];
  tokenPrefix: string;
} {
  let source = preprocessLiquidEmbeds(markdown).replace(CHART_SHELL_PATTERN, "$1");
  let tokenPrefix = "\uE000MEDOUSA_LIQUID_";
  while (source.includes(tokenPrefix)) tokenPrefix += "_";
  const fragments: TrustedFragment[] = [];

  const extract = (pattern: RegExp, block: boolean): void => {
    source = source.replace(pattern, (html) => {
      const token = `${tokenPrefix}${fragments.length}\uE001`;
      fragments.push({ token, html, block });
      return token;
    });
  };

  extract(KANBAN_PATTERN, true);
  extract(EMBED_PATTERN, true);
  extract(ICON_PATTERN, false);
  return { source, fragments, tokenPrefix };
}

function renderProse(
  value: string,
  fragments: TrustedFragment[],
  tokenPrefix: string,
): string {
  const byToken = new Map(fragments.map((fragment) => [fragment.token, fragment]));
  const lines = value.split(/\n/);
  let html = "";
  let list = false;

  const closeList = (): void => {
    if (!list) return;
    html += "</ul>";
    list = false;
  };

  for (const rawLine of lines) {
    const blockFragment = rawLine.trim().startsWith(tokenPrefix)
      ? byToken.get(rawLine.trim())
      : undefined;
    if (blockFragment?.block) {
      closeList();
      html += blockFragment.token;
      continue;
    }

    let line = escapeHtml(rawLine);
    line = line.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label: string, href: string) => {
      const safe = safeUrl(href);
      return safe
        ? `<a href="#" data-href="${escapeHtml(safe)}">${label}</a>`
        : label;
    });
    line = line
      .replace(/\`([^`]+)\`/g, '<code class="inline">$1</code>')
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/__([^_]+)__/g, "<strong>$1</strong>")
      .replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, "<em>$1</em>");

    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    const item = /^[-*]\s+(.+)$/.exec(line);
    if (heading) {
      closeList();
      html += `<h${heading[1]!.length}>${heading[2]}</h${heading[1]!.length}>`;
    } else if (item) {
      if (!list) {
        html += "<ul>";
        list = true;
      }
      html += `<li>${item[1]}</li>`;
    } else {
      closeList();
      if (line.trim()) html += `<p>${line}</p>`;
    }
  }
  closeList();
  return html;
}

/** Render chat Markdown while preserving only placeholders emitted by the shared parser. */
export function renderWebviewMarkdown(value: string): string {
  const { source, fragments, tokenPrefix } = extractTrustedLiquid(value);
  let html = "";
  let cursor = 0;
  const pattern = /```([\w+-]*)\n([\s\S]*?)```/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(source))) {
    html += renderProse(source.slice(cursor, match.index), fragments, tokenPrefix);
    const language = match[1] || "code";
    const code = match[2] ?? "";
    const encoded = encodeURIComponent(code);
    html += `<div class="code-block"><div class="code-head"><span>${escapeHtml(language)}</span><div class="code-actions"><button data-copy-code="${encoded}">Copy</button><button data-insert-code="${encoded}">Insert</button></div></div><pre><code>${escapeHtml(code)}</code></pre></div>`;
    cursor = pattern.lastIndex;
  }
  html += renderProse(source.slice(cursor), fragments, tokenPrefix);

  for (const fragment of fragments) {
    html = html.split(fragment.token).join(fragment.html);
  }
  return html;
}

export const medousaLiquidWebview: MedousaLiquidWebviewApi = {
  renderMarkdown: renderWebviewMarkdown,
  hydrate: hydrateLiquidMarkdown,
};

declare global {
  interface Window {
    medousaLiquidMarkdown?: MedousaLiquidWebviewApi;
  }
}

if (typeof window !== "undefined") {
  window.medousaLiquidMarkdown = medousaLiquidWebview;
}
