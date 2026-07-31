/** M8e — read-only spreadsheet preview (CSV + XLSX first sheet). */

import readXlsxFile from "read-excel-file/browser";

export interface SpreadsheetPreviewData {
  headers: string[];
  rows: string[][];
  sheetName?: string;
  sourcePath: string;
  truncated: boolean;
  totalRows: number;
}

const MAX_PREVIEW_ROWS = 500;
const MAX_PREVIEW_COLS = 32;

const SPREADSHEET_EXTENSIONS = new Set(["csv", "tsv", "xlsx", "xls", "xlsm"]);

export function spreadsheetExtension(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

export function isSpreadsheetPath(path: string): boolean {
  return SPREADSHEET_EXTENSIONS.has(spreadsheetExtension(path));
}

export function normalizeSpreadsheetRows(rows: string[][]): SpreadsheetPreviewData["rows"] {
  const width = Math.min(
    MAX_PREVIEW_COLS,
    rows.reduce((max, row) => Math.max(max, row.length), 0),
  );
  return rows.slice(0, MAX_PREVIEW_ROWS).map((row) => {
    const normalized = row.slice(0, width).map((cell) => String(cell ?? "").trim());
    while (normalized.length < width) normalized.push("");
    return normalized;
  });
}

function stripBom(text: string): string {
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

export function parseDelimitedRecords(text: string, delimiter: string): string[][] {
  const input = stripBom(text);
  const records: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let inQuotes = false;

  for (let i = 0; i < input.length; i += 1) {
    const char = input[i]!;
    const next = input[i + 1];

    if (char === '"') {
      if (inQuotes && next === '"') {
        cell += '"';
        i += 1;
      } else {
        inQuotes = !inQuotes;
      }
      continue;
    }

    if (!inQuotes && char === delimiter) {
      row.push(cell);
      cell = "";
      continue;
    }

    if (!inQuotes && (char === "\n" || char === "\r")) {
      if (char === "\r" && next === "\n") i += 1;
      row.push(cell);
      if (row.some((value) => value.trim().length > 0)) {
        records.push(row);
      }
      row = [];
      cell = "";
      continue;
    }

    cell += char;
  }

  row.push(cell);
  if (row.some((value) => value.trim().length > 0)) {
    records.push(row);
  }

  return records;
}

export function parseCsvSpreadsheet(text: string, sourcePath: string): SpreadsheetPreviewData {
  const delimiter = spreadsheetExtension(sourcePath) === "tsv" ? "\t" : ",";
  const records = parseDelimitedRecords(text, delimiter);
  if (records.length === 0) {
    return {
      headers: ["Column A"],
      rows: [],
      sourcePath,
      truncated: false,
      totalRows: 0,
    };
  }

  const [headerRow, ...bodyRows] = records;
  const headers = headerRow.map((value, index) => value.trim() || `Column ${index + 1}`);
  const normalizedRows = normalizeSpreadsheetRows(bodyRows);
  const totalRows = bodyRows.length;

  return {
    headers: headers.slice(0, MAX_PREVIEW_COLS),
    rows: normalizedRows,
    sourcePath,
    truncated: totalRows > MAX_PREVIEW_ROWS,
    totalRows,
  };
}

function cellToString(value: unknown): string {
  if (value == null) return "";
  if (value instanceof Date) return value.toISOString();
  return String(value);
}

export async function parseXlsxSpreadsheet(
  base64: string,
  sourcePath: string,
): Promise<SpreadsheetPreviewData> {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return parseXlsxBytes(bytes, sourcePath);
}

export async function parseXlsxBytes(
  bytes: Uint8Array,
  sourcePath: string,
): Promise<SpreadsheetPreviewData> {
  const file = new Blob([bytes]);
  let sheets;
  try {
    sheets = await readXlsxFile(file);
  } catch (err) {
    throw new Error(
      `Could not parse spreadsheet "${sourcePath}": ${err instanceof Error ? err.message : String(err)}`,
    );
  }

  const first = sheets[0];
  const sheetName = first?.sheet ?? "Sheet1";
  const matrix = first?.data ?? [];
  if (matrix.length === 0) {
    return {
      headers: ["Column A"],
      rows: [],
      sheetName,
      sourcePath,
      truncated: false,
      totalRows: 0,
    };
  }

  const [headerRow, ...bodyRows] = matrix;
  const headers = (headerRow ?? []).map(
    (value, index) => cellToString(value).trim() || `Column ${index + 1}`,
  );
  const stringRows = bodyRows.map((row) => row.map((value) => cellToString(value)));
  const normalizedRows = normalizeSpreadsheetRows(stringRows);

  return {
    headers: headers.slice(0, MAX_PREVIEW_COLS),
    rows: normalizedRows,
    sheetName,
    sourcePath,
    truncated: bodyRows.length > MAX_PREVIEW_ROWS,
    totalRows: bodyRows.length,
  };
}
