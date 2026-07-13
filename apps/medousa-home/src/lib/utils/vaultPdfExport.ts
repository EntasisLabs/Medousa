import { invoke } from "@tauri-apps/api/core";
import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
import { renderMarkdownPreview } from "$lib/markdown";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { isTauri } from "$lib/window";

const PDF_EXPORT_CSS = `
  .vault-pdf-export-mount,
  .vault-pdf-export-mount * {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }

  .vault-pdf-export-mount {
    background: #ffffff !important;
    color: #111827 !important;
    font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif !important;
    font-size: 14px !important;
    line-height: 1.65 !important;
  }

  .vault-pdf-export-mount h1,
  .vault-pdf-export-mount h2,
  .vault-pdf-export-mount h3,
  .vault-pdf-export-mount h4,
  .vault-pdf-export-mount h5,
  .vault-pdf-export-mount h6 {
    color: #111827 !important;
    font-weight: 600 !important;
  }

  .vault-pdf-export-mount h1 { font-size: 1.5rem !important; margin: 0 0 1rem !important; }
  .vault-pdf-export-mount h2 { font-size: 1.25rem !important; margin: 1.25rem 0 0.5rem !important; }
  .vault-pdf-export-mount h3 { font-size: 1.1rem !important; margin: 1rem 0 0.5rem !important; }

  .vault-pdf-export-mount p,
  .vault-pdf-export-mount li,
  .vault-pdf-export-mount td,
  .vault-pdf-export-mount th,
  .vault-pdf-export-mount blockquote,
  .vault-pdf-export-mount span,
  .vault-pdf-export-mount strong,
  .vault-pdf-export-mount em,
  .vault-pdf-export-mount div {
    color: #111827 !important;
  }

  .vault-pdf-export-mount a {
    color: #2563eb !important;
    text-decoration: underline !important;
  }

  .vault-pdf-export-mount blockquote {
    border-left: 3px solid #d1d5db !important;
    padding-left: 12px !important;
    color: #374151 !important;
  }

  .vault-pdf-export-mount ul { list-style: disc !important; padding-left: 1.25rem !important; }
  .vault-pdf-export-mount ol { list-style: decimal !important; padding-left: 1.25rem !important; }

  .vault-pdf-export-mount table {
    width: 100% !important;
    border-collapse: collapse !important;
    margin: 12px 0 !important;
  }

  .vault-pdf-export-mount th,
  .vault-pdf-export-mount td {
    border: 1px solid #d1d5db !important;
    padding: 6px 10px !important;
  }

  .vault-pdf-export-mount th {
    background: #f3f4f6 !important;
    font-weight: 600 !important;
  }

  .vault-pdf-export-mount .markdown-code-block,
  .vault-pdf-export-mount pre,
  .vault-pdf-export-mount .markdown-pre {
    background: #f3f4f6 !important;
    border: 1px solid #d1d5db !important;
    border-radius: 6px !important;
  }

  .vault-pdf-export-mount code,
  .vault-pdf-export-mount .markdown-code {
    background: #f3f4f6 !important;
    color: #111827 !important;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
  }

  .vault-pdf-export-mount :not(pre) > code {
    padding: 0.1rem 0.35rem !important;
    border-radius: 4px !important;
  }

  .vault-pdf-export-mount .markdown-code-copy,
  .vault-pdf-export-mount .markdown-wikilink {
    display: none !important;
  }

  .vault-pdf-export-mount mark.markdown-highlight {
    background: #fef08a !important;
    color: #422006 !important;
  }

  .vault-pdf-export-mount .markdown-callout {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    border-radius: 6px !important;
    padding: 12px !important;
    margin: 12px 0 !important;
  }

  .vault-pdf-export-mount pre.mermaid {
    background: #f9fafb !important;
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-md-embed {
    margin: 1rem 0 !important;
    break-inside: avoid;
  }

  .vault-pdf-export-mount .liquid-chart {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    color: #111827 !important;
    border-radius: 8px !important;
    padding: 12px !important;
    box-shadow: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-tooltip {
    display: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-title,
  .vault-pdf-export-mount .liquid-chart-center-value,
  .vault-pdf-export-mount .liquid-chart-value-label,
  .vault-pdf-export-mount .liquid-chart-pie-label,
  .vault-pdf-export-mount .liquid-chart-axis,
  .vault-pdf-export-mount .liquid-chart-radar-label,
  .vault-pdf-export-mount .liquid-chart-legend-label {
    color: #111827 !important;
    fill: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-chart-description,
  .vault-pdf-export-mount .liquid-chart-caption,
  .vault-pdf-export-mount .liquid-chart-center-label {
    color: #4b5563 !important;
    fill: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-chart-mount {
    animation: none !important;
  }

  /* Report organism — hex only (html2canvas rejects color-mix) */
  .vault-pdf-export-mount .liquid-report {
    border: 1px solid #d1d5db !important;
    background: #f9fafb !important;
    box-shadow: none !important;
    color: #111827 !important;
    border-radius: 8px !important;
    padding: 14px 16px 16px !important;
  }

  .vault-pdf-export-mount .liquid-report-header {
    border-bottom: 1px solid #e5e7eb !important;
  }

  .vault-pdf-export-mount .liquid-report-title {
    color: #111827 !important;
  }

  .vault-pdf-export-mount .liquid-report-subtitle,
  .vault-pdf-export-mount .liquid-report-body,
  .vault-pdf-export-mount .liquid-report-body .markdown-content,
  .vault-pdf-export-mount .liquid-report-body .markdown-content p {
    color: #374151 !important;
  }

  .vault-pdf-export-mount .liquid-report-body .markdown-content h1,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h2,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h3,
  .vault-pdf-export-mount .liquid-report-body .markdown-content h4 {
    color: #111827 !important;
  }

  /* Heatmap / scatter / combo extras */
  .vault-pdf-export-mount .liquid-chart-heatmap-wrap {
    background: transparent !important;
  }

  .vault-pdf-export-mount .liquid-chart-heatmap-col-label,
  .vault-pdf-export-mount .liquid-chart-heatmap-row-label {
    color: #4b5563 !important;
  }

  .vault-pdf-export-mount .liquid-chart-heatmap-cell {
    border: none !important;
    box-shadow: none !important;
  }

  .vault-pdf-export-mount .liquid-chart-grid {
    stroke: #e5e7eb !important;
  }

  .vault-pdf-export-mount .liquid-chart-axis-right {
    fill: #7c3aed !important;
    color: #7c3aed !important;
  }
`;

const COLOR_STYLE_PROPS = [
  "color",
  "background-color",
  "border-top-color",
  "border-right-color",
  "border-bottom-color",
  "border-left-color",
  "outline-color",
  "fill",
  "stroke",
  "stop-color",
] as const;

/**
 * html2canvas cannot parse `color-mix()` / `color()` — bake resolved rgb/hex
 * onto liquid embeds before capture (browser getComputedStyle already resolves them).
 */
function sanitizeUnsupportedCssColors(root: HTMLElement): void {
  const scope = root.querySelectorAll<Element>(
    ".liquid-report, .liquid-report *, .liquid-chart, .liquid-chart *, .liquid-md-embed, .liquid-md-embed *",
  );
  const nodes: Element[] = [root, ...scope];

  for (const el of nodes) {
    if (!(el instanceof HTMLElement) && !(el instanceof SVGElement)) continue;
    const computed = getComputedStyle(el);

    for (const prop of COLOR_STYLE_PROPS) {
      const value = computed.getPropertyValue(prop).trim();
      if (!value || value === "none") continue;
      try {
        el.style.setProperty(prop, value, "important");
      } catch {
        /* some SVG props reject setProperty in older engines */
      }
    }

    // SVG presentation attributes with literal color-mix / color()
    if (el instanceof SVGElement) {
      for (const attr of ["fill", "stroke", "stop-color"] as const) {
        const raw = el.getAttribute(attr);
        if (!raw || raw === "none" || raw === "currentColor") continue;
        if (!/color-mix\s*\(|(^|\s)color\s*\(/.test(raw)) continue;
        const resolved = computed.getPropertyValue(attr).trim();
        if (resolved && resolved !== "none") {
          el.setAttribute(attr, resolved);
        }
      }
    }

    // Kill shadows that often embed color-mix in component CSS
    if (el instanceof HTMLElement) {
      const shadow = computed.boxShadow;
      if (shadow && shadow !== "none") {
        el.style.setProperty("box-shadow", "none", "important");
      }
    }
  }
}

/** Replace `fnName(...)` with balanced parentheses (handles nested rgb()/var()). */
function replaceBalancedCssFn(input: string, fnName: string, replacement: string): string {
  const openRe = new RegExp(`${fnName}\\s*\\(`, "gi");
  let out = "";
  let last = 0;
  let match: RegExpExecArray | null;
  openRe.lastIndex = 0;
  while ((match = openRe.exec(input))) {
    const start = match.index;
    // Don't treat `color-mix` as `color(`
    if (fnName.toLowerCase() === "color") {
      const prev = input.slice(Math.max(0, start - 4), start);
      if (/mix$/i.test(prev)) continue;
      if (start > 0 && /[a-z-]/i.test(input[start - 1] ?? "")) continue;
    }
    let i = start + match[0].length;
    let depth = 1;
    while (i < input.length && depth > 0) {
      const ch = input[i++];
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
    }
    out += input.slice(last, start) + replacement;
    last = i;
    openRe.lastIndex = i;
  }
  return out + input.slice(last);
}

function stripUnsupportedColorFns(css: string): string {
  let out = replaceBalancedCssFn(css, "color-mix", "transparent");
  out = replaceBalancedCssFn(out, "color", "#111827");
  return out;
}

/** Strip color-mix / color() from cloned stylesheets so html2canvas's parser never sees them. */
function scrubUnsupportedColorFunctionsInClone(doc: Document): void {
  for (const sheet of Array.from(doc.styleSheets)) {
    const owner = sheet.ownerNode;
    if (!(owner instanceof HTMLStyleElement)) continue;
    try {
      const text = owner.textContent ?? "";
      if (/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(text)) {
        owner.textContent = stripUnsupportedColorFns(text);
      }
    } catch {
      /* ignore */
    }
  }

  for (const el of doc.querySelectorAll<HTMLElement | SVGElement>("[style]")) {
    const raw = el.getAttribute("style");
    if (!raw || !/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(raw)) continue;
    el.setAttribute("style", stripUnsupportedColorFns(raw));
  }

  for (const el of doc.querySelectorAll("svg [fill], svg [stroke], svg [stop-color]")) {
    for (const attr of ["fill", "stroke", "stop-color"]) {
      const raw = el.getAttribute(attr);
      if (!raw || !/color-mix\s*\(|(^|[^a-z-])color\s*\(/.test(raw)) continue;
      el.setAttribute(attr, "#64748b");
    }
  }
}

function slugifyFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

async function waitForPaint(): Promise<void> {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

/** Extra settle time for LayerCake / chart layout before capture. */
async function waitForLiquidLayout(): Promise<void> {
  await waitForPaint();
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 80);
  });
}

function buildPdfExportDom(title: string, html: string): {
  shell: HTMLElement;
  mount: HTMLElement;
  bodyEl: HTMLElement;
} {
  const shell = document.createElement("div");
  shell.className = "vault-pdf-export-shell";
  // Off-screen — do not flash a full-screen white "preview" during capture
  shell.style.cssText =
    "position:fixed;left:-10000px;top:0;width:780px;height:auto;overflow:visible;pointer-events:none;visibility:hidden;z-index:-1;";

  const mount = document.createElement("div");
  mount.className = "vault-pdf-export-mount";
  mount.style.cssText = "width:720px;max-width:720px;margin:0;padding:48px 40px 64px;background:#ffffff;";

  const styleEl = document.createElement("style");
  styleEl.textContent = PDF_EXPORT_CSS;

  const titleEl = document.createElement("h1");
  titleEl.textContent = title;

  const bodyEl = document.createElement("div");
  bodyEl.className = "vault-pdf-export-body markdown-content";
  bodyEl.innerHTML = html;

  mount.append(styleEl, titleEl, bodyEl);
  shell.appendChild(mount);

  return { shell, mount, bodyEl };
}

export function vaultPdfFilename(title: string): string {
  return `${slugifyFilename(title)}.pdf`;
}

/** Hydrate note markdown → PDF blob (same bytes Save would write). */
export async function renderVaultNotePdfBlob(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<Blob> {
  const body = stripFrontmatter(options.content).content;
  const html = renderMarkdownPreview(body, options.labelByPath);
  if (!html.trim()) {
    throw new Error("Nothing to export — note preview is empty.");
  }

  const filename = vaultPdfFilename(options.title);
  const { shell, mount, bodyEl } = buildPdfExportDom(options.title, html);
  document.body.appendChild(shell);

  try {
    await hydrateMarkdownContainer(bodyEl, {
      liquidContext: {
        titleByPath: options.labelByPath,
        openLinksInWeb: false,
      },
      code: true,
      mermaid: true,
      liquid: true,
      animate: false,
    });
    await waitForLiquidLayout();
    sanitizeUnsupportedCssColors(mount);

    const html2pdf = (await import("html2pdf.js")).default;
    const worker = html2pdf()
      .set({
        margin: [0.55, 0.6, 0.55, 0.6],
        filename,
        image: { type: "jpeg", quality: 0.96 },
        html2canvas: {
          scale: 2,
          useCORS: true,
          backgroundColor: "#ffffff",
          scrollX: 0,
          scrollY: 0,
          windowWidth: mount.scrollWidth,
          logging: false,
          onclone: (clonedDoc: Document) => {
            scrubUnsupportedColorFunctionsInClone(clonedDoc);
            const clonedMount = clonedDoc.querySelector<HTMLElement>(".vault-pdf-export-mount");
            if (clonedMount) sanitizeUnsupportedCssColors(clonedMount);
          },
        },
        jsPDF: { unit: "in", format: "letter", orientation: "portrait" },
        pagebreak: { mode: ["css", "legacy"] },
      })
      .from(mount);

    return (await worker.outputPdf("blob")) as Blob;
  } finally {
    destroyLiquidEmbeds(bodyEl);
    shell.remove();
  }
}

/** Persist a rendered PDF blob (Tauri save dialog or browser download). Returns false if cancelled. */
export async function saveVaultNotePdfBlob(blob: Blob, filename: string): Promise<boolean> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: filename,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      title: "Export note as PDF",
    });
    if (!path) return false;
    const bytes = new Uint8Array(await blob.arrayBuffer());
    await invoke("write_file_bytes", { path, bytes: Array.from(bytes) });
    return true;
  }

  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
  return true;
}

/** One-shot render + save (no preview). Prefer the preview modal in UI. */
export async function exportVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  const blob = await renderVaultNotePdfBlob(options);
  await saveVaultNotePdfBlob(blob, vaultPdfFilename(options.title));
}

export async function downloadVaultNotePdf(options: {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
}): Promise<void> {
  await exportVaultNotePdf(options);
}
