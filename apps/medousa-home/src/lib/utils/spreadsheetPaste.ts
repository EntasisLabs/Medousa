/**
 * Shared spreadsheet paste/import — clipboard TSV/CSV (Excel, Sheets, Numbers)
 * and .xlsx/.xls files → normalized headers + rows for pipe tables and ledgers.
 *
 * Parsing reuses the preview readers in `spreadsheetPreview.ts`; this module
 * keeps vault editors free of delimiter sniffing and Excel quirks.
 */

import {
  parseCsvSpreadsheet,
  parseDelimitedRecords,
  parseXlsxBytes,
} from "$lib/utils/spreadsheetPreview";

export interface SpreadsheetTableData {
  headers: string[];
  rows: string[][];
}

export type SpreadsheetPasteMode = "tsv" | "csv";

const XLSX_MIMES = new Set([
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.ms-excel",
]);

function defaultHeaders(width: number): string[] {
  const count = Math.max(width, 1);
  return Array.from({ length: count }, (_, index) => `Column ${index + 1}`);
}

function normalizeMatrix(matrix: string[][]): SpreadsheetTableData | null {
  const records = matrix.filter((row) =>
    row.some((cell) => cell.trim().length > 0),
  );
  if (records.length === 0) return null;
  const width = records.reduce((max, row) => Math.max(max, row.length), 0);
  if (width === 0) return null;
  const pad = (row: string[]): string[] => {
    const out = row.map((cell) => cell.trim());
    while (out.length < width) out.push("");
    return out;
  };
  const [first, ...rest] = records;
  const headers = pad(first).map(
    (value, index) => value || `Column ${index + 1}`,
  );
  return { headers, rows: rest.map(pad) };
}

/** Parse pasted TSV/CSV text (Excel/Sheets clipboard). */
export function parseSpreadsheetText(
  text: string,
  mode: SpreadsheetPasteMode = "tsv",
): SpreadsheetTableData | null {
  const delimiter = mode === "tsv" ? "\t" : ",";
  return normalizeMatrix(parseDelimitedRecords(text, delimiter));
}

/** Parse a CSV or TSV file body (from file picker or drop). */
export function parseDelimitedFile(
  text: string,
  filename: string,
): SpreadsheetTableData | null {
  const preview = parseCsvSpreadsheet(text, filename);
  if (preview.rows.length === 0 && preview.headers.length <= 1) return null;
  return { headers: preview.headers, rows: preview.rows };
}

/** Parse .xlsx / .xls bytes (file picker or drop). */
export function parseXlsxFile(
  bytes: Uint8Array,
  filename: string,
): SpreadsheetTableData | null {
  const preview = parseXlsxBytes(bytes, filename);
  if (preview.rows.length === 0 && preview.headers.length <= 1) return null;
  return { headers: preview.headers, rows: preview.rows };
}

export function clipboardHasSpreadsheet(data: DataTransfer): boolean {
  if (data.types.includes("text/plain")) {
    const text = data.getData("text/plain");
    return /\t/.test(text) || /\n/.test(text);
  }
  return false;
}

/**
 * Extract spreadsheet data from a paste event.
 * Prefers Excel HTML tables (preserves grid), then plain text TSV/CSV.
 */
export function spreadsheetDataFromClipboard(
  data: DataTransfer,
): SpreadsheetTableData | null {
  const html = data.getData("text/html");
  if (html) {
    const fromHtml = parseHtmlTable(html);
    if (fromHtml) return fromHtml;
  }
  const text = data.getData("text/plain");
  if (!text.trim()) return null;
  const mode: SpreadsheetPasteMode = /\t/.test(text) ? "tsv" : "csv";
  return parseSpreadsheetText(text, mode);
}

function parseHtmlTable(html: string): SpreadsheetTableData | null {
  if (!/<table[\s>]/i.test(html)) return null;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const table = doc.querySelector("table");
  if (!table) return null;
  const matrix: string[][] = [];
  for (const tr of Array.from(table.querySelectorAll("tr"))) {
    const cells = Array.from(tr.querySelectorAll("th,td")).map((cell) =>
      (cell.textContent ?? "").trim(),
    );
    if (cells.length > 0) matrix.push(cells);
  }
  return normalizeMatrix(matrix);
}

export function isSpreadsheetFile(file: File): boolean {
  if (XLSX_MIMES.has(file.type)) return true;
  return /\.(csv|tsv|xlsx|xls|xlsm)$/i.test(file.name);
}

/** Read a spreadsheet File (csv/tsv/xlsx/xls) into headers + rows. */
export async function spreadsheetDataFromFile(
  file: File,
): Promise<SpreadsheetTableData | null> {
  if (/\.(xlsx|xls|xlsm)$/i.test(file.name) || XLSX_MIMES.has(file.type)) {
    const bytes = new Uint8Array(await file.arrayBuffer());
    return parseXlsxFile(bytes, file.name);
  }
  const text = await file.text();
  return parseDelimitedFile(text, file.name);
}

/** Serialize headers + rows as a markdown pipe table. */
export function pipeTableFromSpreadsheet(data: SpreadsheetTableData): string {
  const headers = data.headers.map((cell) => escapePipeCell(cell));
  const divider = data.headers.map(() => "---");
  const body = data.rows.map((row) =>
    data.headers.map((_, index) => escapePipeCell(row[index] ?? "")),
  );
  const lines = [headers, divider, ...body].map((cells) => `| ${cells.join(" | ")} |`);
  return lines.join("\n");
}

function escapePipeCell(value: string): string {
  return value.replace(/\|/g, "\\|").replace(/\r?\n/g, " ").trim();
}
