/** M7c.2 — parse and serialize markdown pipe tables (ledger view). */

import { normalizeKind, stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { columnDisplayLabel } from "$lib/utils/ledgerSheet";

export interface MarkdownTable {
  headers: string[];
  rows: string[][];
  /** Line index in the original markdown (0-based, includes frontmatter lines). */
  startLine: number;
  endLine: number;
}

export const LEDGER_CORE_HEADERS = ["date", "payee", "amount", "category"] as const;

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
  return columnDisplayLabel(value).trim().toLowerCase();
}

export function isLedgerHeaders(headers: string[]): boolean {
  const norm = headers.map(normalizeHeader);
  return LEDGER_CORE_HEADERS.every((header) => norm.includes(header));
}

function frontmatterKindIsLedger(markdown: string): boolean {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return false;
  for (const line of frontmatter.split("\n")) {
    if (!line.trimStart().startsWith("kind:")) continue;
    const value = line.slice(line.indexOf(":") + 1);
    const kind = normalizeKind(value);
    return kind === "ledger" || kind === "sheet";
  }
  return false;
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
  let firstTable: MarkdownTable | null = null;

  for (let i = 0; i < lines.length - 1; i++) {
    const table = parsePipeTableAt(lines, i);
    if (!table) continue;
    if (!firstTable) firstTable = table;
    if (isLedgerHeaders(table.headers)) return table;
  }

  // Ledger notes may rename/extend columns; still treat the first pipe table as the sheet.
  if (firstTable && frontmatterKindIsLedger(markdown)) {
    return firstTable;
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

export function replaceLedgerTable(
  markdown: string,
  rows: string[][],
  headers?: string[],
): string | null {
  const table = findLedgerTable(markdown);
  if (!table) return null;
  return replaceTableBlock(markdown, table, headers ?? table.headers, rows);
}

export function ledgerHeadersFromContent(markdown: string): string[] {
  const table = findLedgerTable(markdown);
  return table?.headers ?? ["Date", "Payee", "Amount", "Category"];
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

/** True when this column is one of the four ledger core fields (by display label). */
export function isLedgerCoreHeader(header: string): boolean {
  return (LEDGER_CORE_HEADERS as readonly string[]).includes(normalizeHeader(header));
}

/**
 * Columns that must stay: named core headers when present, otherwise the first four.
 * Used to block deleting essential ledger structure.
 */
export function ledgerProtectedColumnIndexes(headers: string[]): Set<number> {
  const protectedIndexes = new Set<number>();
  headers.forEach((header, index) => {
    if (isLedgerCoreHeader(header)) protectedIndexes.add(index);
  });
  if (protectedIndexes.size >= 4) return protectedIndexes;
  for (let i = 0; i < Math.min(4, headers.length); i += 1) {
    protectedIndexes.add(i);
  }
  return protectedIndexes;
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
