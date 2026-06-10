/** M8e — read-only spreadsheet preview (CSV + XLSX first sheet). */

import * as XLSX from "xlsx";

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

function parseDelimitedRecords(text: string, delimiter: string): string[][] {
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

export function parseXlsxSpreadsheet(base64: string, sourcePath: string): SpreadsheetPreviewData {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return parseXlsxBytes(bytes, sourcePath);
}

export function parseXlsxBytes(bytes: Uint8Array, sourcePath: string): SpreadsheetPreviewData {
  const workbook = XLSX.read(bytes, { type: "array" });
  const sheetName = workbook.SheetNames[0] ?? "Sheet1";
  const sheet = workbook.Sheets[sheetName];
  if (!sheet) {
    return {
      headers: ["Column A"],
      rows: [],
      sheetName,
      sourcePath,
      truncated: false,
      totalRows: 0,
    };
  }

  const matrix = XLSX.utils.sheet_to_json<(string | number | boolean | null)[]>(sheet, {
    header: 1,
    defval: "",
    raw: false,
  }) as (string | number | boolean | null)[][];

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
  const headers = (headerRow ?? []).map((value, index) =>
    String(value ?? "").trim() || `Column ${index + 1}`,
  );
  const stringRows = bodyRows.map((row) => row.map((value) => String(value ?? "")));
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
