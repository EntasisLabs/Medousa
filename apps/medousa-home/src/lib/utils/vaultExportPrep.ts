/**
 * Shared export prep: markdown → hydrated print-paper DOM mount.
 * Used by PDF (html2pdf) and Word (DOM walk / snapshots).
 */

import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
import { renderMarkdownPreview } from "$lib/markdown";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import {
  exportContentWidthPx,
  normalizeVaultExportOptions,
  type VaultExportOptions,
} from "./vaultExportOptions";
import { buildExportPrintCss } from "./vaultExportPrintCss";

export interface VaultExportPrepInput {
  title: string;
  content: string;
  labelByPath: Map<string, string>;
  /** Note path for resolving vault-relative images. */
  notePath?: string | null;
  options?: Partial<VaultExportOptions> | null;
}

export interface VaultExportPrepResult {
  options: VaultExportOptions;
  shell: HTMLElement;
  mount: HTMLElement;
  bodyEl: HTMLElement;
  /** Tear down liquid hosts and remove shell from document. */
  dispose: () => void;
}

async function waitForPaint(): Promise<void> {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

async function waitForLiquidLayout(): Promise<void> {
  await waitForPaint();
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 100);
  });
}

/** Expand FAQ / accordion content for print capture. */
export function expandDetailsForExport(root: HTMLElement): void {
  for (const el of root.querySelectorAll("details")) {
    el.open = true;
    el.setAttribute("open", "");
  }
}

/** Strip interactive chrome that should not appear in export. */
export function stripExportChrome(root: HTMLElement): void {
  const kill = root.querySelectorAll(
    [
      ".markdown-code-copy",
      ".liquid-chart-toolbar",
      ".liquid-chart-configure",
      ".liquid-chart-tooltip",
      ".vault-live-quiet-chrome",
      "[data-export-strip]",
    ].join(","),
  );
  for (const el of kill) el.remove();

  for (const btn of root.querySelectorAll(
    ".liquid-compare-entity-btn, .liquid-compare-card",
  )) {
    if (btn instanceof HTMLElement) {
      btn.style.pointerEvents = "none";
      btn.removeAttribute("onclick");
    }
  }
}

/** Ensure tables/embeds don't collapse under constrained page width. */
export function hardenExportLayout(root: HTMLElement): void {
  for (const el of root.querySelectorAll<HTMLElement>(
    "table, .liquid-md-embed, .liquid-compare, .liquid-chart, .liquid-report, .markdown-table-scroll",
  )) {
    el.style.minWidth = "0";
    if (!el.style.width) el.style.width = "100%";
  }
}

/**
 * html2canvas cannot parse `color-mix()` / `color()` — bake resolved rgb/hex
 * onto liquid embeds before capture.
 */
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

export function sanitizeUnsupportedCssColors(root: HTMLElement): void {
  const scope = root.querySelectorAll<Element>(
    ".liquid-report, .liquid-report *, .liquid-chart, .liquid-chart *, .liquid-compare, .liquid-compare *, .liquid-md-embed, .liquid-md-embed *",
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
        /* ignore */
      }
    }

    if (el instanceof SVGElement) {
      for (const attr of ["fill", "stroke", "stop-color"] as const) {
        const raw = el.getAttribute(attr);
        if (!raw || raw === "none" || raw === "currentColor") continue;
        if (!/color-mix\s*\(|(^|\s)color\s*\(/.test(raw)) continue;
        const resolved = computed.getPropertyValue(attr).trim();
        if (resolved && resolved !== "none") el.setAttribute(attr, resolved);
      }
    }

    if (el instanceof HTMLElement) {
      const shadow = computed.boxShadow;
      if (shadow && shadow !== "none") {
        el.style.setProperty("box-shadow", "none", "important");
      }
    }
  }
}

function replaceBalancedCssFn(
  input: string,
  fnName: string,
  replacement: string,
): string {
  const openRe = new RegExp(`${fnName}\\s*\\(`, "gi");
  let out = "";
  let last = 0;
  let match: RegExpExecArray | null;
  openRe.lastIndex = 0;
  while ((match = openRe.exec(input))) {
    const start = match.index;
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

export function scrubUnsupportedColorFunctionsInClone(doc: Document): void {
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

/**
 * Snapshot a DOM node to a PNG data URL (for Word ImageRun / PDF freeze).
 * Returns null if capture fails or node has no size.
 */
export async function snapshotElementToPng(
  el: HTMLElement,
): Promise<{ dataUrl: string; width: number; height: number } | null> {
  const rect = el.getBoundingClientRect();
  const width = Math.max(1, Math.ceil(el.scrollWidth || rect.width));
  const height = Math.max(1, Math.ceil(el.scrollHeight || rect.height));
  if (width < 2 || height < 2) return null;

  try {
    const html2canvas = (await import("html2canvas")).default;
    const canvas = await html2canvas(el, {
      backgroundColor: "#ffffff",
      scale: 2,
      useCORS: true,
      logging: false,
      width,
      height,
      windowWidth: width,
      onclone: (clonedDoc: Document) => {
        scrubUnsupportedColorFunctionsInClone(clonedDoc);
      },
    });
    return {
      dataUrl: canvas.toDataURL("image/png"),
      width: canvas.width,
      height: canvas.height,
    };
  } catch {
    return null;
  }
}

export async function prepareVaultExportMount(
  input: VaultExportPrepInput,
): Promise<VaultExportPrepResult> {
  const options = normalizeVaultExportOptions(input.options);
  const body = stripFrontmatter(input.content).content;
  const html = renderMarkdownPreview(body, {
    titleByPath: input.labelByPath,
    resolveLocalImages: Boolean(input.notePath),
  });
  if (!html.trim()) {
    throw new Error("Nothing to export — note preview is empty.");
  }

  const width = exportContentWidthPx(options);
  const shell = document.createElement("div");
  shell.className = "vault-pdf-export-shell";
  shell.style.cssText =
    `position:fixed;left:-10000px;top:0;width:${width + 60}px;height:auto;overflow:visible;pointer-events:none;visibility:hidden;z-index:-1;`;

  const mount = document.createElement("div");
  mount.className = "vault-pdf-export-mount";
  mount.dataset.exportPaper = "1";
  mount.style.cssText = `width:${width}px;max-width:${width}px;margin:0;padding:48px 40px 64px;background:#ffffff;`;

  const styleEl = document.createElement("style");
  styleEl.textContent = buildExportPrintCss(options);

  const titleEl = document.createElement("h1");
  titleEl.textContent = input.title;

  const bodyEl = document.createElement("div");
  bodyEl.className = "vault-pdf-export-body markdown-content";
  bodyEl.innerHTML = html;

  mount.append(styleEl, titleEl, bodyEl);
  shell.appendChild(mount);
  document.body.appendChild(shell);

  try {
    await hydrateMarkdownContainer(bodyEl, {
      liquidContext: {
        titleByPath: input.labelByPath,
        openLinksInWeb: false,
      },
      localImagePath: input.notePath ?? null,
      code: true,
      mermaid: true,
      liquid: true,
      localImages: Boolean(input.notePath),
      animate: false,
    });
    await waitForLiquidLayout();
    expandDetailsForExport(mount);
    stripExportChrome(mount);
    hardenExportLayout(mount);
    sanitizeUnsupportedCssColors(mount);

    return {
      options,
      shell,
      mount,
      bodyEl,
      dispose: () => {
        destroyLiquidEmbeds(bodyEl);
        shell.remove();
      },
    };
  } catch (err) {
    destroyLiquidEmbeds(bodyEl);
    shell.remove();
    throw err;
  }
}
