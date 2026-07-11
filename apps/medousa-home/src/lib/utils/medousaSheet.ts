/** `medousa-sheet` fence — sheet-level ledger config (filter/sort/title). */

import { findLedgerTable } from "$lib/utils/markdownTable";
import {
  parseViewPredicate,
  type ViewPredicate,
} from "$lib/utils/markdownView";
import {
  columnDisplayLabel,
  type LedgerColumn,
} from "$lib/utils/ledgerSheet";

export interface MedousaSheetSort {
  column: string;
  descending: boolean;
}

export interface MedousaSheetConfig {
  id?: string;
  title?: string;
  filters: ViewPredicate[];
  sort?: MedousaSheetSort;
}

export interface MedousaSheetBlock {
  config: MedousaSheetConfig;
  startLine: number;
  endLine: number;
}

export interface LedgerViewRow {
  sourceIndex: number;
  cells: string[];
}

const SHEET_FENCE_OPEN = /^```medousa-sheet\s*$/i;
const SHEET_FENCE_CLOSE = /^```\s*$/;

export function emptyMedousaSheetConfig(): MedousaSheetConfig {
  return { filters: [] };
}

export function isMedousaSheetConfigEmpty(config: MedousaSheetConfig): boolean {
  return (
    !config.id?.trim() &&
    !config.title?.trim() &&
    config.filters.length === 0 &&
    !config.sort
  );
}

export function parseMedousaSheetBody(body: string): MedousaSheetConfig {
  const config = emptyMedousaSheetConfig();
  const lines = body.replace(/\r\n/g, "\n").trim().split("\n");

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const colon = trimmed.indexOf(":");
    if (colon === -1) continue;
    const key = trimmed.slice(0, colon).trim().toLowerCase();
    const value = trimmed.slice(colon + 1).trim();
    if (!value) continue;

    if (key === "id") {
      config.id = value;
      continue;
    }
    if (key === "title") {
      config.title = value;
      continue;
    }
    if (key === "filter" || key === "where") {
      const predicate = parseViewPredicate(value);
      if (predicate) config.filters.push(predicate);
      continue;
    }
    if (key === "sort") {
      const descending = value.startsWith("-");
      const column = descending ? value.slice(1).trim() : value;
      if (column) {
        config.sort = { column, descending };
      }
    }
  }

  return config;
}

export function serializeMedousaSheetBody(config: MedousaSheetConfig): string {
  const lines: string[] = [];
  if (config.id?.trim()) lines.push(`id: ${config.id.trim()}`);
  if (config.title?.trim()) lines.push(`title: ${config.title.trim()}`);
  for (const filter of config.filters) {
    const value =
      /\s/.test(filter.value) || /["']/.test(filter.value)
        ? `"${filter.value.replace(/"/g, '\\"')}"`
        : filter.value;
    lines.push(`filter: ${filter.column} ${filter.op} ${value}`);
  }
  if (config.sort?.column) {
    lines.push(
      `sort: ${config.sort.descending ? "-" : ""}${config.sort.column}`,
    );
  }
  return lines.join("\n");
}

export function serializeMedousaSheetFence(config: MedousaSheetConfig): string {
  const body = serializeMedousaSheetBody(config);
  return `\`\`\`medousa-sheet\n${body}\n\`\`\``;
}

function findAllSheetBlocks(lines: string[]): MedousaSheetBlock[] {
  const blocks: MedousaSheetBlock[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    if (!SHEET_FENCE_OPEN.test(lines[i].trim())) continue;
    const startLine = i;
    let endLine = -1;
    const bodyLines: string[] = [];
    for (let j = i + 1; j < lines.length; j += 1) {
      if (SHEET_FENCE_CLOSE.test(lines[j].trim())) {
        endLine = j;
        break;
      }
      bodyLines.push(lines[j]);
    }
    if (endLine === -1) continue;
    blocks.push({
      config: parseMedousaSheetBody(bodyLines.join("\n")),
      startLine,
      endLine,
    });
    i = endLine;
  }
  return blocks;
}

function onlyBlankBetween(lines: string[], fromExclusive: number, toExclusive: number): boolean {
  for (let i = fromExclusive + 1; i < toExclusive; i += 1) {
    if (lines[i].trim() !== "") return false;
  }
  return true;
}

/** Sheet fence immediately above the ledger table (blank lines allowed between). */
export function findLedgerSheetBlock(markdown: string): MedousaSheetBlock | null {
  const table = findLedgerTable(markdown);
  if (!table) return null;
  const lines = markdown.split("\n");
  const blocks = findAllSheetBlocks(lines);
  let best: MedousaSheetBlock | null = null;
  for (const block of blocks) {
    if (block.endLine >= table.startLine) continue;
    if (!onlyBlankBetween(lines, block.endLine, table.startLine)) continue;
    if (!best || block.endLine > best.endLine) best = block;
  }
  return best;
}

export function ledgerSheetConfigFromContent(markdown: string): MedousaSheetConfig {
  return findLedgerSheetBlock(markdown)?.config ?? emptyMedousaSheetConfig();
}

export function upsertLedgerSheetFence(
  markdown: string,
  config: MedousaSheetConfig,
): string {
  const table = findLedgerTable(markdown);
  if (!table) return markdown;

  const lines = markdown.split("\n");
  const existing = findLedgerSheetBlock(markdown);
  const empty = isMedousaSheetConfigEmpty(config);

  if (empty) {
    if (!existing) return markdown;
    const before = lines.slice(0, existing.startLine);
    let afterStart = existing.endLine + 1;
    while (afterStart < table.startLine && lines[afterStart]?.trim() === "") {
      afterStart += 1;
    }
    // Keep a single blank line before the table when content remains above.
    const beforeTrimmed = [...before];
    while (
      beforeTrimmed.length > 0 &&
      beforeTrimmed[beforeTrimmed.length - 1].trim() === ""
    ) {
      beforeTrimmed.pop();
    }
    const after = lines.slice(afterStart);
    if (beforeTrimmed.length === 0) return after.join("\n");
    return [...beforeTrimmed, "", ...after].join("\n");
  }

  const fenceLines = serializeMedousaSheetFence(config).split("\n");

  if (existing) {
    const before = lines.slice(0, existing.startLine);
    const after = lines.slice(existing.endLine + 1);
    return [...before, ...fenceLines, ...after].join("\n");
  }

  const before = lines.slice(0, table.startLine);
  const after = lines.slice(table.startLine);
  const beforeTrimmed = [...before];
  while (
    beforeTrimmed.length > 0 &&
    beforeTrimmed[beforeTrimmed.length - 1].trim() === ""
  ) {
    beforeTrimmed.pop();
  }
  if (beforeTrimmed.length === 0) {
    return [...fenceLines, "", ...after].join("\n");
  }
  return [...beforeTrimmed, "", ...fenceLines, "", ...after].join("\n");
}

function columnIndex(headers: string[], name: string): number {
  const norm = name.trim().toLowerCase();
  return headers.findIndex(
    (header) => columnDisplayLabel(header).trim().toLowerCase() === norm,
  );
}

function rowMatchesFilter(
  cells: string[],
  headers: string[],
  predicate: ViewPredicate,
): boolean {
  const index = columnIndex(headers, predicate.column);
  if (index === -1) return false;
  const cell = (cells[index] ?? "").trim().toLowerCase();
  const value = predicate.value.trim().toLowerCase();
  return predicate.op === "=" ? cell === value : cell !== value;
}

/** View-only filter/sort over ledger rows (source order preserved in storage). */
export function applyMedousaSheetView(
  columns: LedgerColumn[],
  rows: string[][],
  config: MedousaSheetConfig,
): LedgerViewRow[] {
  const headers = columns.map((column) => column.label);
  let indexed: LedgerViewRow[] = rows.map((cells, sourceIndex) => ({
    sourceIndex,
    cells,
  }));

  for (const filter of config.filters) {
    indexed = indexed.filter(({ cells }) =>
      rowMatchesFilter(cells, headers, filter),
    );
  }

  if (config.sort?.column) {
    const index = columnIndex(headers, config.sort.column);
    if (index !== -1) {
      const descending = config.sort.descending;
      indexed = [...indexed].sort((left, right) => {
        const cmp = (left.cells[index] ?? "").localeCompare(
          right.cells[index] ?? "",
          undefined,
          { numeric: true, sensitivity: "base" },
        );
        return descending ? -cmp : cmp;
      });
    }
  }

  return indexed;
}
