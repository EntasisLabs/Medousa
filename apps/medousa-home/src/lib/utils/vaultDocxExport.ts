/**
 * Markdown → Word (.docx) export for vault notes.
 * Best-effort fidelity: headings, paragraphs, lists, tables, code fences.
 */

import {
  Document,
  HeadingLevel,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableRow,
  TextRun,
  WidthType,
  type IParagraphOptions,
} from "docx";
import { invoke } from "@tauri-apps/api/core";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { isTauri } from "$lib/window";

function slugifyFilename(title: string): string {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "note";
}

export function vaultDocxFilename(title: string): string {
  return `${slugifyFilename(title)}.docx`;
}

function inlineRuns(text: string): TextRun[] {
  // Strip simple markdown emphasis into plain runs (bold/italic best-effort).
  const runs: TextRun[] = [];
  const re = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|[^*`]+)/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) != null) {
    const chunk = match[1];
    if (chunk.startsWith("**") && chunk.endsWith("**")) {
      runs.push(new TextRun({ text: chunk.slice(2, -2), bold: true }));
    } else if (chunk.startsWith("*") && chunk.endsWith("*")) {
      runs.push(new TextRun({ text: chunk.slice(1, -1), italics: true }));
    } else if (chunk.startsWith("`") && chunk.endsWith("`")) {
      runs.push(new TextRun({ text: chunk.slice(1, -1), font: "Courier New" }));
    } else if (chunk) {
      runs.push(new TextRun({ text: chunk }));
    }
  }
  return runs.length > 0 ? runs : [new TextRun({ text: "" })];
}

function parsePipeRow(line: string): string[] {
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  return trimmed.split("|").map((cell) => cell.trim());
}

function isPipeSep(line: string): boolean {
  return /^\|?[\s:-]+\|[\s|:-]*$/.test(line.trim());
}

function headingLevel(level: number): (typeof HeadingLevel)[keyof typeof HeadingLevel] {
  if (level <= 1) return HeadingLevel.HEADING_1;
  if (level === 2) return HeadingLevel.HEADING_2;
  return HeadingLevel.HEADING_3;
}

export function markdownToDocxChildren(markdown: string): (Paragraph | Table)[] {
  const body = stripFrontmatter(markdown).content.replace(/\r\n/g, "\n");
  const lines = body.split("\n");
  const out: (Paragraph | Table)[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      i += 1;
      continue;
    }

    if (line.trimStart().startsWith("```")) {
      const codeLines: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]);
        i += 1;
      }
      if (i < lines.length) i += 1;
      out.push(
        new Paragraph({
          spacing: { before: 120, after: 120 },
          children: [
            new TextRun({
              text: codeLines.join("\n") || " ",
              font: "Courier New",
              size: 18,
            }),
          ],
        }),
      );
      continue;
    }

    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      out.push(
        new Paragraph({
          heading: headingLevel(heading[1].length),
          children: inlineRuns(heading[2]),
        }),
      );
      i += 1;
      continue;
    }

    if (line.trim().startsWith("|") && i + 1 < lines.length && isPipeSep(lines[i + 1])) {
      const rows: string[][] = [parsePipeRow(line)];
      i += 2;
      while (i < lines.length && lines[i].trim().startsWith("|")) {
        rows.push(parsePipeRow(lines[i]));
        i += 1;
      }
      out.push(
        new Table({
          width: { size: 100, type: WidthType.PERCENTAGE },
          rows: rows.map(
            (cells, rowIndex) =>
              new TableRow({
                children: cells.map(
                  (cell) =>
                    new TableCell({
                      children: [
                        new Paragraph({
                          children: [
                            new TextRun({
                              text: cell || " ",
                              bold: rowIndex === 0,
                            }),
                          ],
                        }),
                      ],
                    }),
                ),
              }),
          ),
        }),
      );
      continue;
    }

    const list = /^(\s*)([-*+]|\d+\.)\s+(.+)$/.exec(line);
    if (list) {
      const bullet = !/^\d/.test(list[2]);
      const opts: IParagraphOptions = {
        children: inlineRuns(list[3]),
      };
      if (bullet) {
        opts.bullet = { level: 0 };
      } else {
        opts.numbering = { reference: "vault-export-num", level: 0 };
      }
      out.push(new Paragraph(opts));
      i += 1;
      continue;
    }

    const task = /^(\s*)-\s+\[([ xX])\]\s+(.*)$/.exec(line);
    if (task) {
      const mark = task[2].toLowerCase() === "x" ? "☑" : "☐";
      out.push(
        new Paragraph({
          children: inlineRuns(`${mark} ${task[3]}`),
        }),
      );
      i += 1;
      continue;
    }

    if (line.trim() === "---" || line.trim() === "***") {
      out.push(new Paragraph({ children: [new TextRun({ text: "—" })] }));
      i += 1;
      continue;
    }

    out.push(new Paragraph({ children: inlineRuns(line) }));
    i += 1;
  }

  if (out.length === 0) {
    out.push(new Paragraph({ children: [new TextRun({ text: "" })] }));
  }
  return out;
}

export async function renderVaultNoteDocxBlob(options: {
  title: string;
  content: string;
}): Promise<Blob> {
  const children = markdownToDocxChildren(options.content);
  const doc = new Document({
    numbering: {
      config: [
        {
          reference: "vault-export-num",
          levels: [
            {
              level: 0,
              format: "decimal",
              text: "%1.",
              alignment: "left",
            },
          ],
        },
      ],
    },
    sections: [
      {
        properties: {},
        children: [
          new Paragraph({
            heading: HeadingLevel.TITLE,
            children: [new TextRun({ text: options.title || "Untitled", bold: true })],
          }),
          ...children,
        ],
      },
    ],
  });
  return Packer.toBlob(doc);
}

export async function saveVaultNoteDocxBlob(
  blob: Blob,
  filename: string,
): Promise<boolean> {
  if (isTauri()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: filename,
      filters: [{ name: "Word", extensions: ["docx"] }],
      title: "Export note as Word",
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

export async function exportVaultNoteDocx(options: {
  title: string;
  content: string;
}): Promise<void> {
  const blob = await renderVaultNoteDocxBlob(options);
  await saveVaultNoteDocxBlob(blob, vaultDocxFilename(options.title));
}
