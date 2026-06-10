/** M7c.2 — parse and serialize markdown pipe tables (ledger view). */

import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

export interface MarkdownTable {
  headers: string[];
  rows: string[][];
  /** Line index in the original markdown (0-based, includes frontmatter lines). */
  startLine: number;
  endLine: number;
}

const LEDGER_HEADERS = ["date", "payee", "amount", "category"];

function splitPipeRow(line: string): string[] | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("|") || !trimmed.endsWith("|")) return null;
  return trimmed
    .slice(1, -1)
    .split("|")
    .map((cell) => cell.trim());
}

function isSeparatorRow(cells: string[]): boolean {
  return cells.every((cell) => /^:?-+:?$/.test(cell.replace(/\s/g, "")));
}

function normalizeHeader(value: string): string {
  return value.trim().toLowerCase();
}

export function isLedgerHeaders(headers: string[]): boolean {
  const norm = headers.map(normalizeHeader);
  return LEDGER_HEADERS.every((header) => norm.includes(header));
}

export function parsePipeTableAt(lines: string[], start: number): MarkdownTable | null {
  const headerCells = splitPipeRow(lines[start] ?? "");
  if (!headerCells || headerCells.length < 2) return null;

  const separatorCells = splitPipeRow(lines[start + 1] ?? "");
  if (!separatorCells || !isSeparatorRow(separatorCells)) return null;

  const rows: string[][] = [];
  let end = start + 1;
  for (let i = start + 2; i < lines.length; i++) {
    const cells = splitPipeRow(lines[i] ?? "");
    if (!cells) break;
    rows.push(cells);
    end = i;
  }

  return {
    headers: headerCells,
    rows,
    startLine: start,
    endLine: end,
  };
}

export function findFirstPipeTable(markdown: string): MarkdownTable | null {
  const lines = markdown.split("\n");
  for (let i = 0; i < lines.length - 1; i++) {
    const table = parsePipeTableAt(lines, i);
    if (table) return table;
  }
  return null;
}

export function findLedgerTable(markdown: string): MarkdownTable | null {
  const lines = markdown.split("\n");
  for (let i = 0; i < lines.length - 1; i++) {
    const table = parsePipeTableAt(lines, i);
    if (table && isLedgerHeaders(table.headers)) return table;
  }
  return null;
}

export function serializePipeTable(headers: string[], rows: string[][]): string {
  const headerLine = `| ${headers.join(" | ")} |`;
  const separator = `| ${headers.map(() => "---").join(" | ")} |`;
  const body = rows.map((row) => {
    const padded = headers.map((_, index) => row[index] ?? "");
    return `| ${padded.join(" | ")} |`;
  });
  return [headerLine, separator, ...body].join("\n");
}

export function replaceTableBlock(
  markdown: string,
  table: MarkdownTable,
  headers: string[],
  rows: string[][],
): string {
  const lines = markdown.split("\n");
  const replacement = serializePipeTable(headers, rows);
  const before = lines.slice(0, table.startLine);
  const after = lines.slice(table.endLine + 1);
  return [...before, ...replacement.split("\n"), ...after].join("\n");
}

export function replaceLedgerTable(markdown: string, rows: string[][]): string | null {
  const table = findLedgerTable(markdown);
  if (!table) return null;
  return replaceTableBlock(markdown, table, table.headers, rows);
}

export function ledgerRowsFromContent(markdown: string): string[][] {
  const table = findLedgerTable(markdown);
  if (!table) {
    return [["", "", "", ""]];
  }
  if (table.rows.length === 0) {
    return [table.headers.map(() => "")];
  }
  return table.rows.map((row) =>
    table.headers.map((_, index) => row[index] ?? ""),
  );
}

export function tableToCsv(table: MarkdownTable): string {
  const escape = (value: string) => {
    if (/[",\n]/.test(value)) {
      return `"${value.replace(/"/g, '""')}"`;
    }
    return value;
  };
  const lines = [
    table.headers.map(escape).join(","),
    ...table.rows.map((row) =>
      table.headers.map((_, index) => escape(row[index] ?? "")).join(","),
    ),
  ];
  return lines.join("\n");
}

export function ledgerCsvFromContent(markdown: string): string | null {
  const table = findLedgerTable(markdown);
  return table ? tableToCsv(table) : null;
}

/** Body without frontmatter — for preview snippets around tables. */
export function markdownBody(markdown: string): string {
  return stripFrontmatter(markdown).content;
}

export function contentWithoutLedgerTable(markdown: string): string {
  const table = findLedgerTable(markdown);
  if (!table) return markdownBody(markdown);
  const lines = markdown.split("\n");
  const before = lines.slice(0, table.startLine).join("\n").trimEnd();
  const after = lines.slice(table.endLine + 1).join("\n").trimStart();
  return [before, after].filter(Boolean).join("\n\n");
}
