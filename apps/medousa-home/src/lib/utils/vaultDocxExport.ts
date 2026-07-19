/**
 * Vault note → Word (.docx) via shared export prep + DOM walk.
 */

import {
  AlignmentType,
  BorderStyle,
  Document,
  HeadingLevel,
  ImageRun,
  LevelFormat,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableLayoutType,
  TableRow,
  TextRun,
  WidthType,
  ExternalHyperlink,
  PageBreak,
  type IParagraphOptions,
  type IRunOptions,
} from "docx";
import {
  exportDocxContentWidthDxa,
  exportDocxFontName,
  exportMarginTwips,
  normalizeVaultExportOptions,
  saveExportBlob,
  vaultExportFilename,
  type VaultExportOptions,
} from "./vaultExportOptions";
import {
  bodyHasMatchingTitleH1,
  isLabelLikeParagraph,
  prepareVaultExportMount,
  snapshotElementToPng,
} from "./vaultExportPrep";

export function vaultDocxFilename(title: string): string {
  return vaultExportFilename(title, "docx");
}

type DocxChild = Paragraph | Table;

interface InlineCtx {
  font: string;
  mono: string;
  size: number;
  bold?: boolean;
  italics?: boolean;
}

function halfPoints(px: number): number {
  return Math.round(px * 1.5);
}

function dataUrlToUint8Array(dataUrl: string): Uint8Array {
  const comma = dataUrl.indexOf(",");
  const b64 = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function headingLevel(
  level: number,
): (typeof HeadingLevel)[keyof typeof HeadingLevel] {
  if (level <= 1) return HeadingLevel.HEADING_1;
  if (level === 2) return HeadingLevel.HEADING_2;
  if (level === 3) return HeadingLevel.HEADING_3;
  if (level === 4) return HeadingLevel.HEADING_4;
  if (level === 5) return HeadingLevel.HEADING_5;
  return HeadingLevel.HEADING_6;
}

function isElement(node: Node): node is HTMLElement {
  return node.nodeType === Node.ELEMENT_NODE;
}

function textOf(node: Node): string {
  return (node.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** Explicit ink so Word theme blue does not paint headings. */
export const DOCX_HEADING_COLOR = "111827";

function runOpts(
  text: string,
  ctx: InlineCtx,
  extra?: Partial<IRunOptions>,
): IRunOptions {
  return {
    text,
    font: ctx.font,
    size: ctx.size,
    bold: ctx.bold,
    italics: ctx.italics,
    ...extra,
  };
}

function inlineFromNode(node: Node, ctx: InlineCtx): (TextRun | ExternalHyperlink)[] {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = node.textContent ?? "";
    if (!text) return [];
    return [new TextRun(runOpts(text, ctx))];
  }
  if (!isElement(node)) return [];

  const tag = node.tagName.toLowerCase();
  if (tag === "br") return [new TextRun(runOpts("\n", ctx))];
  if (tag === "strong" || tag === "b") {
    return [...node.childNodes].flatMap((c) =>
      inlineFromNode(c, { ...ctx, bold: true }),
    );
  }
  if (tag === "em" || tag === "i") {
    return [...node.childNodes].flatMap((c) =>
      inlineFromNode(c, { ...ctx, italics: true }),
    );
  }
  if (tag === "code") {
    return [
      new TextRun(
        runOpts(node.textContent ?? "", { ...ctx, font: "Courier New", size: Math.max(16, ctx.size - 2) }),
      ),
    ];
  }
  if (tag === "a" || node.classList.contains("markdown-wikilink")) {
    const href = node.getAttribute("href") ?? "";
    const label = textOf(node) || href;
    if (href.startsWith("http://") || href.startsWith("https://")) {
      return [
        new ExternalHyperlink({
          children: [
            new TextRun(
              runOpts(label, ctx, {
                color: "2563EB",
                underline: {},
              }),
            ),
          ],
          link: href,
        }),
      ];
    }
    return [
      new TextRun(
        runOpts(label, ctx, {
          color: "2563EB",
          underline: {},
        }),
      ),
    ];
  }
  if (tag === "mark") {
    const text = node.textContent ?? "";
    return [
      new TextRun(runOpts(text, ctx, { highlight: "yellow" })),
    ];
  }

  return [...node.childNodes].flatMap((c) => inlineFromNode(c, ctx));
}

function paragraphFromElement(
  el: HTMLElement,
  ctx: InlineCtx,
  extras?: Partial<IParagraphOptions>,
): Paragraph {
  const children = [...el.childNodes].flatMap((c) => inlineFromNode(c, ctx));
  return new Paragraph({
    spacing: { after: 80, line: 240 },
    ...extras,
    children: children.length > 0 ? children : [new TextRun(runOpts("", ctx))],
  });
}

/** Equal column widths in DXA that sum exactly to contentWidth. */
export function buildDocxColumnWidths(
  columnCount: number,
  contentWidthDxa: number,
): number[] {
  const cols = Math.max(1, columnCount);
  const base = Math.floor(contentWidthDxa / cols);
  const widths = Array.from({ length: cols }, () => base);
  const drift = contentWidthDxa - base * cols;
  widths[cols - 1] += drift;
  return widths;
}

function tableFromElement(
  table: HTMLElement,
  ctx: InlineCtx,
  contentWidthDxa: number,
): Table {
  const rows = [...table.querySelectorAll("tr")];
  const colCount = Math.max(
    1,
    ...rows.map((tr) => tr.querySelectorAll("th, td").length),
  );
  const columnWidths = buildDocxColumnWidths(colCount, contentWidthDxa);
  const cellBorders = {
    top: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
    bottom: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
    left: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
    right: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
  };

  return new Table({
    width: { size: contentWidthDxa, type: WidthType.DXA },
    columnWidths,
    layout: TableLayoutType.FIXED,
    rows: rows.map((tr, rowIndex) => {
      const cells = [...tr.querySelectorAll("th, td")];
      return new TableRow({
        children: columnWidths.map((colW, colIndex) => {
          const cell = cells[colIndex];
          return new TableCell({
            borders: cellBorders,
            width: { size: colW, type: WidthType.DXA },
            children: [
              new Paragraph({
                children: [
                  new TextRun(
                    runOpts(cell ? textOf(cell) || " " : " ", {
                      ...ctx,
                      bold:
                        rowIndex === 0 ||
                        (cell != null && cell.tagName === "TH"),
                    }),
                  ),
                ],
              }),
            ],
          });
        }),
      });
    }),
  });
}

function listItems(
  list: HTMLElement,
  ctx: InlineCtx,
  ordered: boolean,
  level: number,
): Paragraph[] {
  const out: Paragraph[] = [];
  for (const li of list.children) {
    if (!(li instanceof HTMLElement) || li.tagName !== "LI") continue;
    const checkbox = li.querySelector('input[type="checkbox"]');
    let prefix = "";
    if (checkbox instanceof HTMLInputElement) {
      prefix = checkbox.checked ? "☑ " : "☐ ";
    }
    const clone = li.cloneNode(true) as HTMLElement;
    for (const nested of clone.querySelectorAll("ul, ol")) nested.remove();
    for (const input of clone.querySelectorAll("input")) input.remove();
    const runs = [...clone.childNodes].flatMap((c) => inlineFromNode(c, ctx));
    if (prefix) {
      runs.unshift(new TextRun(runOpts(prefix, ctx)));
    }
    const listLevel = Math.min(level, 2);
    const opts: IParagraphOptions = !checkbox
      ? ordered
        ? {
            spacing: { after: 40, line: 240 },
            children: runs.length > 0 ? runs : [new TextRun(runOpts("", ctx))],
            numbering: { reference: "vault-export-num", level: listLevel },
          }
        : {
            spacing: { after: 40, line: 240 },
            // Built-in bullets often lack indent; pin left like numbered lists.
            indent: { left: 720 + listLevel * 360, hanging: 360 },
            children: runs.length > 0 ? runs : [new TextRun(runOpts("", ctx))],
            bullet: { level: listLevel },
          }
      : {
          spacing: { after: 40, line: 240 },
          children: runs.length > 0 ? runs : [new TextRun(runOpts("", ctx))],
        };
    out.push(new Paragraph(opts));

    for (const nested of li.children) {
      if (!(nested instanceof HTMLElement)) continue;
      const t = nested.tagName.toLowerCase();
      if (t === "ul" || t === "ol") {
        out.push(...listItems(nested, ctx, t === "ol", level + 1));
      }
    }
  }
  return out;
}

/** CSS selector for Word PNG freeze targets (every liquid embed + static boards). */
export const DOCX_SNAPSHOT_SELECTOR = [
  ".liquid-md-embed",
  ".liquid-mini-kanban",
  "pre.mermaid",
  ".liquid-compare",
  ".liquid-chart",
  ".liquid-report",
].join(", ");

/**
 * Collect unique snapshot roots.
 * Prefer `.vault-export-section` (heading + embed) so Word cannot orphan
 * "Compare" / "Price story" above a tall PNG — keepNext alone is not enough.
 */
export function selectDocxSnapshotTargets(root: HTMLElement): HTMLElement[] {
  const seen = new Set<HTMLElement>();
  const targets: HTMLElement[] = [];

  const markNested = (host: HTMLElement) => {
    seen.add(host);
    for (const nested of host.querySelectorAll<HTMLElement>(
      `${DOCX_SNAPSHOT_SELECTOR}, .vault-export-section`,
    )) {
      seen.add(nested);
    }
  };

  // 1) Glued heading+embed sections — bake the heading into the image.
  for (const section of root.querySelectorAll<HTMLElement>(".vault-export-section")) {
    if (seen.has(section)) continue;
    markNested(section);
    targets.push(section);
  }

  // 2) Remaining liquid hosts not already covered by a section.
  for (const el of root.querySelectorAll<HTMLElement>(DOCX_SNAPSHOT_SELECTOR)) {
    if (seen.has(el)) continue;
    if (el.closest(".vault-export-section")) continue;
    if (el.classList.contains("liquid-md-embed")) {
      markNested(el);
      targets.push(el);
      continue;
    }
    if (el.closest(".liquid-md-embed")) continue;
    seen.add(el);
    targets.push(el);
  }
  return targets;
}

async function collectSnapshots(
  root: HTMLElement,
): Promise<Map<HTMLElement, { data: Uint8Array; width: number; height: number }>> {
  const map = new Map<
    HTMLElement,
    { data: Uint8Array; width: number; height: number }
  >();
  for (const el of selectDocxSnapshotTargets(root)) {
    const snap = await snapshotElementToPng(el);
    if (snap) {
      map.set(el, {
        data: dataUrlToUint8Array(snap.dataUrl),
        width: snap.width,
        height: snap.height,
      });
    }
  }
  return map;
}

function imageParagraph(
  snap: { data: Uint8Array; width: number; height: number },
): Paragraph {
  const maxW = 540;
  const scale = snap.width > maxW ? maxW / snap.width : 1;
  const w = Math.round(snap.width * scale);
  const h = Math.round(snap.height * scale);
  return new Paragraph({
    spacing: { before: 120, after: 120 },
    children: [
      new ImageRun({
        data: snap.data,
        transformation: { width: w, height: h },
        type: "png",
      }),
    ],
  });
}

/**
 * Walk prepared export HTML into docx children.
 * `snapshots` maps rich liquid roots (or their embed hosts) to PNG bytes.
 */
export function htmlExportToDocxChildren(
  root: HTMLElement,
  options: VaultExportOptions,
  snapshots: Map<
    HTMLElement,
    { data: Uint8Array; width: number; height: number }
  > = new Map(),
): DocxChild[] {
  const font = exportDocxFontName(options.fontFamily);
  const mono = "Courier New";
  const size = halfPoints(options.baseFontPx);
  const ctx: InlineCtx = { font, mono, size };
  const contentWidthDxa = exportDocxContentWidthDxa(options);
  const out: DocxChild[] = [];

  const walk = (parent: HTMLElement) => {
    for (const node of parent.childNodes) {
      if (node.nodeType === Node.TEXT_NODE) {
        const t = (node.textContent ?? "").trim();
        if (t) {
          out.push(
            new Paragraph({
              spacing: { after: 80, line: 240 },
              children: [new TextRun(runOpts(t, ctx))],
            }),
          );
        }
        continue;
      }
      if (!isElement(node)) continue;

      if (node.classList.contains("vault-export-page-break")) {
        out.push(new Paragraph({ children: [new PageBreak()] }));
        continue;
      }

      const snap = snapshots.get(node);
      if (snap) {
        out.push(imageParagraph(snap));
        continue;
      }

      // Skip style tags / hidden chrome
      const tag = node.tagName.toLowerCase();
      if (tag === "style" || tag === "script") continue;

      if (
        node.classList.contains("liquid-compare") ||
        node.classList.contains("liquid-chart") ||
        node.classList.contains("liquid-report") ||
        node.classList.contains("liquid-md-embed") ||
        node.classList.contains("liquid-mini-kanban") ||
        node.classList.contains("liquid-callout") ||
        node.classList.contains("liquid-accordion") ||
        node.classList.contains("liquid-card") ||
        node.classList.contains("liquid-tabs")
      ) {
        const nestedSnap =
          snapshots.get(node) ??
          [...snapshots.entries()].find(([el]) => node.contains(el) || el.contains(node))?.[1];
        if (nestedSnap) {
          out.push(imageParagraph(nestedSnap));
          continue;
        }
        // Plain prose fallback — never italic bracket dumps.
        const fallback = textOf(node);
        if (fallback) {
          const chunk = fallback.length > 1200 ? `${fallback.slice(0, 1200)}…` : fallback;
          out.push(
            new Paragraph({
              spacing: { after: 80, line: 240 },
              children: [new TextRun(runOpts(chunk, ctx))],
            }),
          );
        }
        continue;
      }

      // Glued section already snapshotted as one image (heading baked in).
      if (node.classList.contains("vault-export-section")) {
        const sectionSnap =
          snapshots.get(node) ??
          [...snapshots.entries()].find(([el]) => node.contains(el))?.[1];
        if (sectionSnap) {
          out.push(imageParagraph(sectionSnap));
          continue;
        }
        walk(node);
        continue;
      }

      if (/^h[1-6]$/.test(tag)) {
        // If this heading lives inside a snapshotted section, skip — it's in the PNG.
        const section = node.closest(".vault-export-section");
        if (section && snapshots.has(section)) continue;

        const level = Number(tag[1]);
        const headingCtx = {
          ...ctx,
          size: halfPoints(options.baseFontPx * (level === 1 ? 1.5 : level === 2 ? 1.25 : 1.1)),
          bold: true,
        };
        const nextEl = node.nextElementSibling as HTMLElement | null;
        const nextIsSnap = Boolean(
          nextEl &&
            (snapshots.has(nextEl) ||
              [...snapshots.keys()].some(
                (el) => nextEl === el || nextEl.contains(el),
              )),
        );
        // keepNext alone fails when the following PNG is taller than the
        // remaining page — force the heading onto the next page with its body.
        out.push(
          new Paragraph({
            heading: headingLevel(level),
            keepNext: true,
            keepLines: true,
            pageBreakBefore: nextIsSnap && level >= 2,
            spacing: { before: level === 1 ? 0 : 160, after: 80, line: 240 },
            children: [
              new TextRun(
                runOpts(textOf(node) || " ", headingCtx, {
                  color: DOCX_HEADING_COLOR,
                }),
              ),
            ],
          }),
        );
        continue;
      }

      if (tag === "p") {
        const labelKeep = isLabelLikeParagraph(node)
          ? { keepNext: true, keepLines: true }
          : undefined;
        out.push(paragraphFromElement(node, ctx, labelKeep));
        continue;
      }

      if (node.classList.contains("vault-export-label-group")) {
        walk(node);
        continue;
      }

      if (tag === "blockquote" || node.classList.contains("markdown-callout")) {
        out.push(
          paragraphFromElement(node, { ...ctx, italics: true }, {
            indent: { left: 420 },
            border: {
              left: { style: BorderStyle.SINGLE, size: 24, color: "D1D5DB", space: 8 },
            },
          }),
        );
        continue;
      }

      if (tag === "pre" || node.classList.contains("markdown-code-block")) {
        const code = node.textContent ?? "";
        out.push(
          new Paragraph({
            spacing: { before: 120, after: 120 },
            shading: { type: "clear", fill: "F3F4F6" },
            children: [
              new TextRun({
                text: code || " ",
                font: mono,
                size: Math.max(16, size - 2),
              }),
            ],
          }),
        );
        continue;
      }

      if (tag === "ul" || tag === "ol") {
        out.push(...listItems(node, ctx, tag === "ol", 0));
        continue;
      }

      if (tag === "table") {
        out.push(tableFromElement(node, ctx, contentWidthDxa));
        continue;
      }

      if (tag === "div" && node.classList.contains("markdown-table-scroll")) {
        const table = node.querySelector("table");
        if (table) out.push(tableFromElement(table, ctx, contentWidthDxa));
        continue;
      }

      if (tag === "hr") {
        out.push(
          new Paragraph({
            spacing: { before: 120, after: 120 },
            children: [new TextRun(runOpts("—", ctx))],
          }),
        );
        continue;
      }

      if (tag === "details") {
        const summary = node.querySelector("summary");
        if (summary) {
          out.push(
            new Paragraph({
              spacing: { before: 120, after: 60 },
              children: [
                new TextRun(runOpts(textOf(summary), { ...ctx, bold: true })),
              ],
            }),
          );
        }
        const body = node.cloneNode(true) as HTMLElement;
        body.querySelector("summary")?.remove();
        walk(body);
        continue;
      }

      if (tag === "img") {
        const src = node.getAttribute("src") ?? "";
        if (src.startsWith("data:image/")) {
          try {
            const data = dataUrlToUint8Array(src);
            const w = Number(node.getAttribute("width")) || 480;
            const h = Number(node.getAttribute("height")) || 320;
            out.push(
              new Paragraph({
                spacing: { before: 120, after: 120 },
                children: [
                  new ImageRun({
                    data,
                    transformation: {
                      width: Math.min(540, w),
                      height: Math.round((Math.min(540, w) / w) * h),
                    },
                    type: src.includes("png") ? "png" : "jpg",
                  }),
                ],
              }),
            );
          } catch {
            /* skip broken image */
          }
        }
        continue;
      }

      // Generic container — walk children
      if (tag === "div" || tag === "section" || tag === "article") {
        walk(node);
        continue;
      }

      const t = textOf(node);
      if (t) {
        out.push(
          new Paragraph({
            spacing: { after: 120 },
            children: [new TextRun(runOpts(t, ctx))],
          }),
        );
      }
    }
  };

  walk(root);

  if (out.length === 0) {
    out.push(new Paragraph({ children: [new TextRun(runOpts("", ctx))] }));
  }
  return out;
}

/** @deprecated Prefer htmlExportToDocxChildren after prep — kept for simple fixtures. */
export function markdownToDocxChildren(markdown: string): DocxChild[] {
  const options = normalizeVaultExportOptions(null);
  if (typeof document === "undefined") {
    // Node/vitest without DOM — minimal fallback
    const lines = markdown.replace(/\r\n/g, "\n").split("\n");
    const out: DocxChild[] = [];
    for (const line of lines) {
      if (!line.trim()) continue;
      const h = /^(#{1,3})\s+(.+)$/.exec(line);
      if (h) {
        out.push(
          new Paragraph({
            heading: headingLevel(h[1].length),
            children: [new TextRun({ text: h[2], bold: true })],
          }),
        );
        continue;
      }
      out.push(new Paragraph({ children: [new TextRun({ text: line })] }));
    }
    return out.length ? out : [new Paragraph({ children: [new TextRun({ text: "" })] })];
  }
  const root = document.createElement("div");
  // Very small marked-free path for tests: wrap lines
  root.innerHTML = markdown
    .split("\n")
    .map((line) => {
      const h = /^(#{1,6})\s+(.+)$/.exec(line);
      if (h) return `<h${h[1].length}>${h[2]}</h${h[1].length}>`;
      if (line.trim().startsWith("|")) return `<p>${line}</p>`;
      return line.trim() ? `<p>${line}</p>` : "";
    })
    .join("");
  // Better: use a tiny table parser for fixtures
  const tableMatch = markdown.match(
    /\|(.+)\|\n\|[\s:-|]+\|\n((?:\|.+\|\n?)*)/,
  );
  if (tableMatch) {
    const headers = tableMatch[1].split("|").map((c) => c.trim()).filter(Boolean);
    const bodyRows = tableMatch[2]
      .trim()
      .split("\n")
      .map((row) =>
        row
          .replace(/^\|/, "")
          .replace(/\|$/, "")
          .split("|")
          .map((c) => c.trim()),
      );
    const table = document.createElement("table");
    const thead = document.createElement("tr");
    for (const h of headers) {
      const th = document.createElement("th");
      th.textContent = h;
      thead.appendChild(th);
    }
    table.appendChild(thead);
    for (const cells of bodyRows) {
      const tr = document.createElement("tr");
      for (const c of cells) {
        const td = document.createElement("td");
        td.textContent = c;
        tr.appendChild(td);
      }
      table.appendChild(tr);
    }
    root.innerHTML = "";
    const h = /^(#{1,3})\s+(.+)$/m.exec(markdown);
    if (h) {
      const el = document.createElement(`h${h[1].length}`);
      el.textContent = h[2];
      root.appendChild(el);
    }
    root.appendChild(table);
  }
  return htmlExportToDocxChildren(root, options);
}

export async function renderVaultNoteDocxBlob(options: {
  title: string;
  content: string;
  labelByPath?: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<Blob> {
  const exportOptions = normalizeVaultExportOptions(options.exportOptions);
  const labels = options.labelByPath ?? new Map<string, string>();

  // In non-DOM test environments, fall back to line parser
  if (typeof document === "undefined") {
    const children = markdownToDocxChildren(options.content);
    const doc = buildDocument(options.title, children, exportOptions);
    return Packer.toBlob(doc);
  }

  const prepared = await prepareVaultExportMount({
    title: options.title,
    content: options.content,
    labelByPath: labels,
    notePath: options.notePath,
    options: exportOptions,
  });

  try {
    const snapshots = await collectSnapshots(prepared.mount);
    const children = htmlExportToDocxChildren(
      prepared.bodyEl,
      exportOptions,
      snapshots,
    );
    const doc = buildDocument(options.title, children, exportOptions, {
      includeTitle: !bodyHasMatchingTitleH1(prepared.bodyEl, options.title),
    });
    return Packer.toBlob(doc);
  } finally {
    prepared.dispose();
  }
}

function buildDocument(
  title: string,
  children: DocxChild[],
  options: VaultExportOptions,
  flags: { includeTitle?: boolean } = {},
): Document {
  const font = exportDocxFontName(options.fontFamily);
  const size = halfPoints(options.baseFontPx);
  const margins = exportMarginTwips(options.margins);
  const includeTitle = flags.includeTitle !== false;
  const page =
    options.pageSize === "a4"
      ? { width: 11906, height: 16838 }
      : { width: 12240, height: 15840 };
  const landscape = options.orientation === "landscape";

  const headingColor = DOCX_HEADING_COLOR;
  const headingDefs = [
    { id: "Heading1", name: "Heading 1", level: 1, scale: 1.5, before: 0, after: 80 },
    { id: "Heading2", name: "Heading 2", level: 2, scale: 1.25, before: 160, after: 60 },
    { id: "Heading3", name: "Heading 3", level: 3, scale: 1.1, before: 140, after: 60 },
    { id: "Heading4", name: "Heading 4", level: 4, scale: 1.05, before: 120, after: 40 },
    { id: "Heading5", name: "Heading 5", level: 5, scale: 1, before: 100, after: 40 },
    { id: "Heading6", name: "Heading 6", level: 6, scale: 1, before: 100, after: 40 },
  ] as const;

  return new Document({
    styles: {
      default: {
        document: {
          run: { font, size, color: headingColor },
          paragraph: { spacing: { after: 80, line: 240 } },
        },
      },
      paragraphStyles: [
        {
          id: "Title",
          name: "Title",
          basedOn: "Normal",
          next: "Normal",
          quickStyle: true,
          run: {
            font,
            size: halfPoints(options.baseFontPx * 1.6),
            bold: true,
            color: headingColor,
          },
          paragraph: {
            spacing: { after: 120, line: 240 },
            keepNext: true,
            keepLines: true,
          },
        },
        ...headingDefs.map((h) => ({
          id: h.id,
          name: h.name,
          basedOn: "Normal",
          next: "Normal",
          quickStyle: true,
          run: {
            font,
            size: halfPoints(options.baseFontPx * h.scale),
            bold: true,
            color: headingColor,
          },
          paragraph: {
            spacing: { before: h.before, after: h.after, line: 240 },
            keepNext: true,
            keepLines: true,
          },
        })),
      ],
    },
    numbering: {
      config: [
        {
          reference: "vault-export-num",
          levels: [0, 1, 2].map((level) => ({
            level,
            format: LevelFormat.DECIMAL,
            text: `%${level + 1}.`,
            alignment: AlignmentType.LEFT,
            start: 1,
            // Without indent, Word parks numbered body text in a tiny
            // right-edge column (character-wrap "squish").
            style: {
              paragraph: {
                indent: {
                  left: 720 + level * 360,
                  hanging: 360,
                },
              },
            },
          })),
        },
      ],
    },
    sections: [
      {
        properties: {
          page: {
            size: landscape
              ? { width: page.height, height: page.width }
              : page,
            margin: margins,
          },
        },
        children: [
          ...(includeTitle
            ? [
                new Paragraph({
                  heading: HeadingLevel.TITLE,
                  keepNext: true,
                  keepLines: true,
                  children: [
                    new TextRun({
                      text: title || "Untitled",
                      bold: true,
                      font,
                      size: halfPoints(options.baseFontPx * 1.6),
                      color: headingColor,
                    }),
                  ],
                }),
              ]
            : []),
          ...children,
        ],
      },
    ],
  });
}

export async function saveVaultNoteDocxBlob(
  blob: Blob,
  filename: string,
): Promise<boolean> {
  return saveExportBlob(blob, filename, "docx");
}

export async function exportVaultNoteDocx(options: {
  title: string;
  content: string;
  labelByPath?: Map<string, string>;
  notePath?: string | null;
  exportOptions?: Partial<VaultExportOptions> | null;
}): Promise<void> {
  const blob = await renderVaultNoteDocxBlob(options);
  await saveVaultNoteDocxBlob(blob, vaultDocxFilename(options.title));
}
