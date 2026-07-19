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
    setTimeout(resolve, 140);
  });
}

/** Normalize heading text for title dedupe. */
export function normalizeExportTitle(text: string): string {
  return text.replace(/\s+/g, " ").trim().toLowerCase();
}

/**
 * True when the rendered body already opens with an h1 matching the note title.
 */
export function bodyHasMatchingTitleH1(
  bodyEl: HTMLElement,
  title: string,
): boolean {
  const first = bodyEl.querySelector(":scope > h1");
  if (!first) return false;
  return normalizeExportTitle(first.textContent ?? "") === normalizeExportTitle(title);
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
    ".liquid-compare-entity-btn, .liquid-compare-card, .liquid-accordion-trigger, .liquid-tabs-tab, .liquid-card-main",
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
    "table, .liquid-md-embed, .liquid-compare, .liquid-chart, .liquid-report, .markdown-table-scroll, .liquid-accordion, .liquid-card, .liquid-callout",
  )) {
    el.style.minWidth = "0";
    if (!el.style.width) el.style.width = "100%";
  }
}

/**
 * Classify organisms for page flow:
 * - Heading+embed sections / compare: keep whole unit unless taller than a page
 *   (fixes orphan h2 + cropped compare sliver at page bottom)
 * - Other short organisms: keep when keepTogether or always for brief/tabs
 */
export function markTallEmbedsForPageFlow(
  root: HTMLElement,
  pageContentHeightPx = 1000,
  keepTogether = false,
): void {
  const pageFitAt = pageContentHeightPx * 0.92;
  const shortAt = pageContentHeightPx * 0.42;
  for (const el of root.querySelectorAll<HTMLElement>(
    [
      ".vault-export-section",
      ".liquid-md-embed",
      ".liquid-compare",
      ".liquid-report",
      ".liquid-accordion",
      ".liquid-brief",
      ".liquid-tabs",
      ".liquid-carousel",
      ".liquid-cite",
    ].join(", "),
  )) {
    const h = el.scrollHeight;
    el.classList.remove("vault-export-allow-break", "vault-export-keep");

    // Sections (h2+embed) and compare matrices: never start mid-page as a sliver.
    if (
      el.classList.contains("vault-export-section") ||
      el.classList.contains("liquid-compare") ||
      el.matches('.liquid-md-embed[data-liquid-embed="compare"]')
    ) {
      if (h > pageFitAt) el.classList.add("vault-export-allow-break");
      else if (h > 0) el.classList.add("vault-export-keep");
      continue;
    }

    if (h > pageFitAt) {
      el.classList.add("vault-export-allow-break");
      continue;
    }
    const alwaysKeep = el.matches(
      ".liquid-brief, .liquid-tabs, .liquid-cite, .liquid-callout, .liquid-carousel",
    );
    if (h > 0 && h <= shortAt && (keepTogether || alwaysKeep)) {
      el.classList.add("vault-export-keep");
    }
  }
}

/**
 * Wrap markdown h2/h3 + following liquid embed so the heading cannot
 * sit alone at the bottom of a page while the organism starts on the next.
 */
function isExportGlueEmbed(el: HTMLElement): boolean {
  return (
    el.classList.contains("liquid-md-embed") ||
    el.classList.contains("liquid-compare") ||
    el.classList.contains("liquid-brief") ||
    el.classList.contains("liquid-tabs") ||
    el.classList.contains("liquid-report") ||
    el.classList.contains("liquid-carousel") ||
    el.classList.contains("liquid-callout") ||
    el.classList.contains("liquid-accordion") ||
    el.classList.contains("liquid-cite")
  );
}

export function glueHeadingsToFollowingEmbed(bodyEl: HTMLElement): void {
  const kids = [...bodyEl.children];
  for (let i = 0; i < kids.length - 1; i++) {
    const cur = kids[i];
    if (!(cur instanceof HTMLElement)) continue;
    if (!/^H[23]$/.test(cur.tagName)) continue;
    if (cur.parentElement?.classList.contains("vault-export-section")) continue;

    // Skip empty <p> so "Compare\\n\\n```compare" still glues.
    const skipped: HTMLElement[] = [];
    let next: HTMLElement | null = null;
    for (let j = i + 1; j < kids.length; j++) {
      const cand = kids[j];
      if (!(cand instanceof HTMLElement)) continue;
      if (cand.tagName === "P" && !(cand.textContent ?? "").trim()) {
        skipped.push(cand);
        continue;
      }
      next = cand;
      break;
    }
    if (!next || !isExportGlueEmbed(next)) continue;

    const wrap = document.createElement("div");
    wrap.className = "vault-export-section";
    cur.before(wrap);
    wrap.append(cur, ...skipped, next);
  }
}

/** True for short bold label lines like "**Anchors:**" / "**Nails:**". */
export function isLabelLikeParagraph(el: HTMLElement): boolean {
  if (el.tagName !== "P") return false;
  const text = (el.textContent ?? "").replace(/\s+/g, " ").trim();
  if (!text || text.length > 80 || !text.endsWith(":")) return false;
  const strong = el.querySelector("strong, b");
  if (!strong) return false;
  const strongText = (strong.textContent ?? "").replace(/\s+/g, " ").trim();
  return strongText === text || strongText === text.replace(/:$/, "") + ":";
}

/**
 * Glue bold label paragraphs to the following list/table so they are not
 * orphaned at the bottom of a PDF page ("Anchors:" alone).
 */
export function glueLabelParagraphsToFollowing(bodyEl: HTMLElement): void {
  const kids = [...bodyEl.children];
  for (let i = 0; i < kids.length - 1; i++) {
    const cur = kids[i];
    if (!(cur instanceof HTMLElement) || !isLabelLikeParagraph(cur)) continue;
    if (
      cur.parentElement?.classList.contains("vault-export-label-group") ||
      cur.parentElement?.classList.contains("vault-export-section")
    ) {
      continue;
    }
    const next = kids[i + 1];
    if (!(next instanceof HTMLElement)) continue;
    const tag = next.tagName;
    const isFollow =
      tag === "UL" ||
      tag === "OL" ||
      tag === "TABLE" ||
      next.classList.contains("markdown-table-scroll");
    if (!isFollow) continue;
    const wrap = document.createElement("div");
    wrap.className = "vault-export-label-group";
    cur.before(wrap);
    wrap.append(cur, next);
  }
}

/**
 * Ensure markdown tables have a real `<thead>` so PDF continuation pages
 * can repeat the header row (`display: table-header-group`).
 */
export function ensureTableHeadersForExport(root: HTMLElement): void {
  for (const table of root.querySelectorAll("table")) {
    if (table.classList.contains("liquid-compare-table")) continue;
    if (table.querySelector("thead")) continue;
    const firstRow = table.querySelector("tr");
    if (!firstRow) continue;
    const hasTh = firstRow.querySelector("th") != null;
    if (!hasTh) continue;
    const thead = document.createElement("thead");
    firstRow.parentElement?.insertBefore(thead, firstRow);
    thead.appendChild(firstRow);
  }
}

/** Shrink wide compare matrices so html2canvas does not clip columns. */
export function densifyCompareForExport(root: HTMLElement): void {
  for (const table of root.querySelectorAll<HTMLElement>(".liquid-compare-table")) {
    table.style.setProperty("width", "100%", "important");
    table.style.setProperty("min-width", "0", "important");
    table.style.setProperty("max-width", "100%", "important");
    table.style.setProperty("table-layout", "fixed", "important");
  }
  for (const scroll of root.querySelectorAll<HTMLElement>(".liquid-compare-scroll")) {
    scroll.style.setProperty("overflow", "visible", "important");
    scroll.style.setProperty("overflow-x", "visible", "important");
    scroll.style.setProperty("max-width", "100%", "important");
  }
  for (const cell of root.querySelectorAll<HTMLElement>(
    ".liquid-compare-corner, .liquid-compare-axis, .liquid-compare-entity, .liquid-compare-cell",
  )) {
    cell.style.setProperty("min-width", "0", "important");
    cell.style.setProperty("max-width", "none", "important");
    cell.style.setProperty("width", "auto", "important");
    cell.style.setProperty("white-space", "normal", "important");
    cell.style.setProperty("overflow-wrap", "anywhere", "important");
    cell.style.setProperty("word-break", "break-word", "important");
  }
}

/**
 * After sanitize bakes computed colors, force paper ink/bg on organisms that
 * often keep dark-theme leftovers (callout / accordion / card).
 */
export function applyPaperColorsAfterSanitize(root: HTMLElement): void {
  const PAPER_BG = "#f9fafb";
  const PAPER_INK = "#111827";
  const PAPER_MUTED = "#374151";
  const PAPER_BORDER = "#d1d5db";
  const WHITE = "#ffffff";

  for (const el of root.querySelectorAll<HTMLElement>(
    ".liquid-callout, .liquid-accordion, .liquid-card, .liquid-tabs, .liquid-brief, .liquid-cite, .markdown-callout",
  )) {
    el.style.setProperty("background", PAPER_BG, "important");
    el.style.setProperty("background-image", "none", "important");
    el.style.setProperty("color", PAPER_INK, "important");
    el.style.setProperty("border-color", PAPER_BORDER, "important");
    el.style.setProperty("box-shadow", "none", "important");
  }

  for (const el of root.querySelectorAll<HTMLElement>(
    [
      ".liquid-callout *",
      ".liquid-accordion-title",
      ".liquid-accordion-label",
      ".liquid-accordion-panel",
      ".liquid-accordion-panel *",
      ".liquid-card-title",
      ".liquid-card-body",
      ".liquid-card-subtitle",
      ".liquid-tabs-title",
      ".liquid-tabs-panel",
      ".liquid-tabs-panel *",
      ".markdown-callout *",
    ].join(","),
  )) {
    el.style.setProperty("color", PAPER_INK, "important");
  }

  for (const el of root.querySelectorAll<HTMLElement>(
    ".liquid-accordion-subtitle, .liquid-card-subtitle, .liquid-tabs-subtitle, .liquid-tabs-export-label, .liquid-accordion-chevron",
  )) {
    el.style.setProperty("color", PAPER_MUTED, "important");
  }

  for (const el of root.querySelectorAll<HTMLElement>(
    ".liquid-accordion-item, .liquid-accordion-panel, .liquid-card, .liquid-tabs-panel--export",
  )) {
    el.style.setProperty("background", WHITE, "important");
    el.style.setProperty("background-image", "none", "important");
  }

  for (const el of root.querySelectorAll<HTMLElement>(
    ".liquid-accordion-trigger, .liquid-card-main",
  )) {
    el.style.setProperty("background", WHITE, "important");
    el.style.setProperty("color", PAPER_INK, "important");
  }

  // Body prose — theme gray on li/em must not survive on white paper.
  for (const el of root.querySelectorAll<HTMLElement>(
    [
      ".vault-pdf-export-body p",
      ".vault-pdf-export-body li",
      ".vault-pdf-export-body em",
      ".vault-pdf-export-body strong",
      ".vault-pdf-export-body ul",
      ".vault-pdf-export-body ol",
      ".vault-pdf-export-body td",
      ".vault-pdf-export-body th",
    ].join(", "),
  )) {
    if (el.closest("a, .markdown-wikilink")) continue;
    el.style.setProperty("color", PAPER_INK, "important");
  }
}

/**
 * html2canvas cannot parse `color-mix()` / `color()` — bake resolved rgb/hex
 * onto liquid embeds before capture. Skips paper-themed organisms so dark ink
 * is not locked in before applyPaperColorsAfterSanitize.
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

function isPaperManagedOrganism(el: Element): boolean {
  return Boolean(
    el.closest(
      ".liquid-callout, .liquid-accordion, .liquid-card, .liquid-tabs, .markdown-callout",
    ),
  );
}

export function sanitizeUnsupportedCssColors(root: HTMLElement): void {
  const scope = root.querySelectorAll<Element>(
    ".liquid-report, .liquid-report *, .liquid-chart, .liquid-chart *, .liquid-compare, .liquid-compare *, .liquid-md-embed, .liquid-md-embed *",
  );
  const nodes: Element[] = [root, ...scope];

  for (const el of nodes) {
    if (!(el instanceof HTMLElement) && !(el instanceof SVGElement)) continue;
    if (isPaperManagedOrganism(el) && el !== root) continue;
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
 * Temporarily reveals off-screen export mounts so html2canvas can measure.
 */
export async function snapshotElementToPng(
  el: HTMLElement,
): Promise<{ dataUrl: string; width: number; height: number } | null> {
  const shell = el.closest(".vault-pdf-export-shell") as HTMLElement | null;
  const prevShell = shell
    ? {
        visibility: shell.style.visibility,
        opacity: shell.style.opacity,
        pointerEvents: shell.style.pointerEvents,
      }
    : null;

  if (shell) {
    shell.style.visibility = "visible";
    shell.style.opacity = "0";
    shell.style.pointerEvents = "none";
  }

  try {
    await waitForPaint();
    const attempt = async () => {
      const rect = el.getBoundingClientRect();
      const width = Math.max(1, Math.ceil(el.scrollWidth || rect.width));
      const height = Math.max(1, Math.ceil(el.scrollHeight || rect.height));
      if (width < 2 || height < 2) return null;

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
    };

    const first = await attempt();
    if (first) return first;
    await new Promise<void>((r) => setTimeout(r, 80));
    return await attempt();
  } catch {
    return null;
  } finally {
    if (shell && prevShell) {
      shell.style.visibility = prevShell.visibility;
      shell.style.opacity = prevShell.opacity;
      shell.style.pointerEvents = prevShell.pointerEvents;
    }
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

  const bodyEl = document.createElement("div");
  bodyEl.className = "vault-pdf-export-body markdown-content";
  bodyEl.innerHTML = html;

  mount.append(styleEl, bodyEl);

  // Inject title only when the note body does not already start with the same H1.
  if (!bodyHasMatchingTitleH1(bodyEl, input.title)) {
    const titleEl = document.createElement("h1");
    titleEl.textContent = input.title;
    mount.insertBefore(titleEl, bodyEl);
  }

  shell.appendChild(mount);
  document.body.appendChild(shell);

  try {
    await hydrateMarkdownContainer(bodyEl, {
      liquidContext: {
        titleByPath: input.labelByPath,
        openLinksInWeb: false,
        exportPaper: true,
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
    glueHeadingsToFollowingEmbed(bodyEl);
    glueLabelParagraphsToFollowing(bodyEl);
    ensureTableHeadersForExport(mount);
    densifyCompareForExport(mount);
    markTallEmbedsForPageFlow(mount, 1000, options.keepTogether);
    sanitizeUnsupportedCssColors(mount);
    applyPaperColorsAfterSanitize(mount);

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
