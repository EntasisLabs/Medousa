import { readExternalFile } from "$lib/utils/externalDeskApi";
import {
  parseCsvSpreadsheet,
  parseXlsxSpreadsheet,
  spreadsheetExtension,
  type SpreadsheetPreviewData,
} from "$lib/utils/spreadsheetPreview";
import { isTauri } from "$lib/window";

export async function loadSpreadsheetPreview(path: string): Promise<SpreadsheetPreviewData> {
  if (!isTauri()) {
    throw new Error("Spreadsheet preview needs the Medousa desktop app and a local file path.");
  }

  const payload = await readExternalFile(path);
  const ext = spreadsheetExtension(path);

  if (ext === "csv" || ext === "tsv") {
    return parseCsvSpreadsheet(payload.content, path);
  }

  if (payload.kind === "base64") {
    return parseXlsxSpreadsheet(payload.content, path);
  }

  throw new Error(`Unsupported spreadsheet format: .${ext || "unknown"}`);
}
