/**
 * Vault note → Word (.docx) via shared export prep + DOM walk.
 */

import {
  BorderStyle,
  Document,
  HeadingLevel,
  ImageRun,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
  ExternalHyperlink,
  PageBreak,
  type IParagraphOptions,
  type IRunOptions,
} from "docx";
import {
  exportDocxFontName,
  exportMarginTwips,
  normalizeVaultExportOptions,
  saveExportBlob,
  vaultExportFilename,
  type VaultExportOptions,
} from "./vaultExportOptions";
import {
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
    spacing: { after: 120 },
    ...extras,
    children: children.length > 0 ? children : [new TextRun(runOpts("", ctx))],
  });
}

function tableFromElement(table: HTMLElement, ctx: InlineCtx): Table {
  const rows = [...table.querySelectorAll("tr")];
  return new Table({
    width: { size: 100, type: WidthType.PERCENTAGE },
    rows: rows.map((tr, rowIndex) => {
      const cells = [...tr.querySelectorAll("th, td")];
      return new TableRow({
        children: cells.map(
          (cell) =>
            new TableCell({
              borders: {
                top: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
                bottom: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
                left: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
                right: { style: BorderStyle.SINGLE, size: 4, color: "D1D5DB" },
              },
              children: [
                new Paragraph({
                  children: [
                    new TextRun(
                      runOpts(textOf(cell) || " ", {
                        ...ctx,
                        bold: rowIndex === 0 || cell.tagName === "TH",
                      }),
                    ),
                  ],
                }),
              ],
            }),
        ),
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
            spacing: { after: 60 },
            children: runs.length > 0 ? runs : [new TextRun(runOpts("", ctx))],
            numbering: { reference: "vault-export-num", level: listLevel },
          }
        : {
            spacing: { after: 60 },
            children: runs.length > 0 ? runs : [new TextRun(runOpts("", ctx))],
            bullet: { level: listLevel },
          }
      : {
          spacing: { after: 60 },
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

function isRichSnapshotTarget(el: HTMLElement): boolean {
  if (el.classList.contains("liquid-compare")) return true;
  if (el.classList.contains("liquid-chart")) return true;
  if (el.classList.contains("liquid-report")) return true;
  if (el.classList.contains("mermaid") || el.tagName === "PRE" && el.classList.contains("mermaid"))
    return true;
  if (el.classList.contains("liquid-md-embed")) {
    const kind = el.dataset.liquidEmbed ?? "";
    return ["chart", "compare", "report", "kanban"].includes(kind);
  }
  return false;
}

async function collectSnapshots(
  root: HTMLElement,
): Promise<Map<HTMLElement, { data: Uint8Array; width: number; height: number }>> {
  const map = new Map<
    HTMLElement,
    { data: Uint8Array; width: number; height: number }
  >();
  const candidates = [
    ...root.querySelectorAll<HTMLElement>(
      ".liquid-compare, .liquid-chart, .liquid-report, pre.mermaid, .liquid-md-embed[data-liquid-embed]",
    ),
  ];
  // Prefer outer liquid-md-embed when it wraps an organism
  const seen = new Set<HTMLElement>();
  for (const el of candidates) {
    if (seen.has(el)) continue;
    if (el.classList.contains("liquid-md-embed")) {
      const inner = el.querySelector<HTMLElement>(
        ".liquid-compare, .liquid-chart, .liquid-report",
      );
      const target = inner ?? el;
      if (!isRichSnapshotTarget(target) && !inner) continue;
      if (seen.has(target)) continue;
      seen.add(target);
      seen.add(el);
      const snap = await snapshotElementToPng(target);
      if (snap) {
        map.set(el, {
          data: dataUrlToUint8Array(snap.dataUrl),
          width: snap.width,
          height: snap.height,
        });
      }
      continue;
    }
    if (el.closest(".liquid-md-embed")) continue;
    seen.add(el);
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
  const out: DocxChild[] = [];

  const walk = (parent: HTMLElement) => {
    for (const node of parent.childNodes) {
      if (node.nodeType === Node.TEXT_NODE) {
        const t = (node.textContent ?? "").trim();
        if (t) {
          out.push(
            new Paragraph({
              spacing: { after: 120 },
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
        node.classList.contains("liquid-md-embed")
      ) {
        const nestedSnap =
          snapshots.get(node) ??
          [...snapshots.entries()].find(([el]) => node.contains(el))?.[1];
        if (nestedSnap) {
          out.push(imageParagraph(nestedSnap));
          continue;
        }
        const fallback = textOf(node);
        if (fallback) {
          out.push(
            new Paragraph({
              spacing: { after: 120 },
              children: [new TextRun(runOpts(`[${fallback.slice(0, 80)}]`, ctx, { italics: true }))],
            }),
          );
        }
        continue;
      }

      if (/^h[1-6]$/.test(tag)) {
        const level = Number(tag[1]);
        const headingCtx = {
          ...ctx,
          size: halfPoints(options.baseFontPx * (level === 1 ? 1.5 : level === 2 ? 1.25 : 1.1)),
          bold: true,
        };
        out.push(
          new Paragraph({
            heading: headingLevel(level),
            spacing: { before: level === 1 ? 0 : 200, after: 120 },
            children: [...node.childNodes].flatMap((c) =>
              inlineFromNode(c, headingCtx),
            ),
          }),
        );
        continue;
      }

      if (tag === "p") {
        out.push(paragraphFromElement(node, ctx));
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
        out.push(tableFromElement(node, ctx));
        continue;
      }

      if (tag === "div" && node.classList.contains("markdown-table-scroll")) {
        const table = node.querySelector("table");
        if (table) out.push(tableFromElement(table, ctx));
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
      includeTitle: true,
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

  return new Document({
    styles: {
      default: {
        document: {
          run: { font, size },
          paragraph: { spacing: { after: 120, line: 276 } },
        },
      },
    },
    numbering: {
      config: [
        {
          reference: "vault-export-num",
          levels: [0, 1, 2].map((level) => ({
            level,
            format: "decimal" as const,
            text: `%${level + 1}.`,
            alignment: "left" as const,
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
                  children: [
                    new TextRun({
                      text: title || "Untitled",
                      bold: true,
                      font,
                      size: halfPoints(options.baseFontPx * 1.6),
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
