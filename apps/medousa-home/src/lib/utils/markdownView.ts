/** Phase F — `medousa-view` query blocks over GFM tables in vault notes. */

import { escapeAttr, escapeHtml } from "$lib/markdown/escape";
import {
  findFirstPipeTable,
  findLedgerTable,
  type MarkdownTable,
} from "$lib/utils/markdownTable";
import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
import { normalizeVaultNotePath } from "$lib/utils/vaultNoteTitle";
import { resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
import type { VaultNote } from "$lib/types/vault";

export interface ViewPredicate {
  column: string;
  op: "=" | "!=";
  value: string;
}

export interface MedousaViewQuery {
  from: string;
  table: "first" | "ledger";
  wheres: ViewPredicate[];
  sort?: { column: string; descending: boolean };
  columns?: string[];
}

export interface ResolvedViewTable {
  headers: string[];
  rows: string[][];
  sourcePath: string;
  sourceLabel: string;
  query: MedousaViewQuery;
}

const VIEW_BLOCK_RE = /```medousa-view\s*\n([\s\S]*?)```/gi;

export function hasMedousaViewBlocks(source: string): boolean {
  return /```medousa-view/i.test(source);
}

export function parseViewPredicate(clause: string): ViewPredicate | null {
  const trimmed = clause.trim();
  const match = trimmed.match(/^([^\s=!<>]+)\s*(!=|=)\s*(.+)$/);
  if (!match) return null;
  let value = match[3].trim();
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    value = value.slice(1, -1);
  }
  return {
    column: match[1].trim(),
    op: match[2] as "=" | "!=",
    value,
  };
}

export function parseViewBlockBody(body: string): MedousaViewQuery | null {
  const lines = body.replace(/\r\n/g, "\n").trim().split("\n");
  const values: Record<string, string> = {};
  const wheres: ViewPredicate[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const colon = trimmed.indexOf(":");
    if (colon === -1) continue;
    const key = trimmed.slice(0, colon).trim().toLowerCase();
    const value = trimmed.slice(colon + 1).trim();
    if (key === "where") {
      const predicate = parseViewPredicate(value);
      if (predicate) wheres.push(predicate);
      continue;
    }
    values[key] = value;
  }

  const from = values.from?.trim();
  if (!from) return null;

  const tableRaw = (values.table ?? "first").trim().toLowerCase();
  const table = tableRaw === "ledger" ? "ledger" : "first";

  let sort: MedousaViewQuery["sort"];
  const sortRaw = values.sort?.trim();
  if (sortRaw) {
    const descending = sortRaw.startsWith("-");
    sort = {
      column: descending ? sortRaw.slice(1).trim() : sortRaw,
      descending,
    };
  }

  let columns: string[] | undefined;
  const columnsRaw = values.columns?.trim();
  if (columnsRaw) {
    columns = columnsRaw
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);
  }

  return { from, table, wheres, sort, columns };
}

/** Serialize a view query to fence body lines (no fences). */
export function serializeMedousaViewQuery(query: MedousaViewQuery): string {
  const lines: string[] = [`from: ${query.from.trim()}`, `table: ${query.table}`];
  for (const where of query.wheres) {
    const needsQuotes = /\s/.test(where.value) || /["']/.test(where.value);
    const value = needsQuotes
      ? `"${where.value.replace(/"/g, '\\"')}"`
      : where.value;
    lines.push(`where: ${where.column} ${where.op} ${value}`);
  }
  if (query.sort?.column) {
    lines.push(
      `sort: ${query.sort.descending ? "-" : ""}${query.sort.column}`,
    );
  }
  if (query.columns?.length) {
    lines.push(`columns: ${query.columns.join(", ")}`);
  }
  return lines.join("\n");
}

/** Full fenced block ready to insert into a note. */
export function serializeMedousaViewFence(query: MedousaViewQuery): string {
  return `\`\`\`medousa-view\n${serializeMedousaViewQuery(query)}\n\`\`\`\n\n`;
}

/** Replace the Nth `medousa-view` fence (0-based) with an updated query. */
export function replaceMedousaViewFenceAt(
  source: string,
  index: number,
  query: MedousaViewQuery,
): string | null {
  const re = /```medousa-view\s*\n([\s\S]*?)```/gi;
  let current = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(source)) !== null) {
    if (current === index) {
      const replacement = `\`\`\`medousa-view\n${serializeMedousaViewQuery(query)}\n\`\`\``;
      return (
        source.slice(0, match.index) +
        replacement +
        source.slice(match.index + match[0].length)
      );
    }
    current += 1;
  }
  return null;
}

export function extractMedousaViewBlocks(
  source: string,
): Array<{ fullMatch: string; body: string; query: MedousaViewQuery | null }> {
  const blocks: Array<{ fullMatch: string; body: string; query: MedousaViewQuery | null }> =
    [];
  for (const match of source.matchAll(VIEW_BLOCK_RE)) {
    const body = match[1] ?? "";
    blocks.push({
      fullMatch: match[0],
      body,
      query: parseViewBlockBody(body),
    });
  }
  return blocks;
}

export function resolveViewSourcePath(
  from: string,
  sourcePath: string | null,
  notes: VaultNote[],
): string | null {
  const trimmed = from.trim();
  if (!trimmed) return null;
  if (trimmed.includes("/")) {
    return normalizeVaultNotePath(trimmed);
  }
  const resolved = resolveWikilinkTarget(trimmed, sourcePath, notes);
  return resolved ?? normalizeVaultNotePath(trimmed);
}

import { columnDisplayLabel } from "$lib/utils/ledgerSheet";

function columnIndex(headers: string[], name: string): number {
  const norm = name.trim().toLowerCase();
  return headers.findIndex(
    (header) => columnDisplayLabel(header).trim().toLowerCase() === norm,
  );
}

export function findTableForView(markdown: string, mode: "first" | "ledger"): MarkdownTable | null {
  const body = stripFrontmatter(markdown).content;
  return mode === "ledger" ? findLedgerTable(body) : findFirstPipeTable(body);
}

function rowMatches(row: string[], headers: string[], predicate: ViewPredicate): boolean {
  const index = columnIndex(headers, predicate.column);
  if (index === -1) return false;
  const cell = (row[index] ?? "").trim().toLowerCase();
  const value = predicate.value.trim().toLowerCase();
  return predicate.op === "=" ? cell === value : cell !== value;
}

function sortRows(
  rows: string[][],
  headers: string[],
  sort: MedousaViewQuery["sort"],
): string[][] {
  if (!sort) return rows;
  const index = columnIndex(headers, sort.column);
  if (index === -1) return rows;
  return [...rows].sort((left, right) => {
    const cmp = (left[index] ?? "").localeCompare(right[index] ?? "", undefined, {
      numeric: true,
      sensitivity: "base",
    });
    return sort.descending ? -cmp : cmp;
  });
}

function projectColumns(
  headers: string[],
  rows: string[][],
  columns?: string[],
): { headers: string[]; rows: string[][] } {
  const displayHeaders = headers.map((header) => columnDisplayLabel(header));
  if (!columns?.length) return { headers: displayHeaders, rows };
  const indices = columns
    .map((name) => ({ name, index: columnIndex(headers, name) }))
    .filter((entry) => entry.index >= 0);
  if (indices.length === 0) return { headers: displayHeaders, rows };
  return {
    headers: indices.map((entry) => displayHeaders[entry.index]),
    rows: rows.map((row) => indices.map((entry) => row[entry.index] ?? "")),
  };
}

export function applyMedousaViewQuery(
  table: MarkdownTable,
  query: MedousaViewQuery,
): { headers: string[]; rows: string[][] } {
  let rows = table.rows.filter((row) =>
    query.wheres.every((predicate) => rowMatches(row, table.headers, predicate)),
  );
  rows = sortRows(rows, table.headers, query.sort);
  return projectColumns(table.headers, rows, query.columns);
}

export function renderMedousaViewError(message: string, viewIndex?: number): string {
  const editAttr =
    viewIndex != null ? ` data-edit-view-index="${escapeAttr(String(viewIndex))}"` : "";
  const configure =
    viewIndex != null
      ? `<button type="button" class="medousa-view-configure" data-edit-view-index="${escapeAttr(String(viewIndex))}">Configure</button>`
      : "";
  return `<div class="medousa-view medousa-view-error" role="note"${editAttr}><p class="medousa-view-error-text">${escapeHtml(message)}</p>${configure}</div>`;
}

function csvEscape(value: string): string {
  if (/[",\n]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

function rowsToCsv(headers: string[], rows: string[][]): string {
  const lines = [
    headers.map(csvEscape).join(","),
    ...rows.map((row) => row.map((cell) => csvEscape(cell ?? "")).join(",")),
  ];
  return lines.join("\n");
}

export function renderMedousaViewTable(
  resolved: ResolvedViewTable,
  viewIndex?: number,
): string {
  const { headers, rows, sourcePath, sourceLabel } = resolved;
  const headerCells = headers
    .map((header) => `<th scope="col">${escapeHtml(header)}</th>`)
    .join("");
  const bodyRows =
    rows.length > 0
      ? rows
          .map(
            (row) =>
              `<tr>${row.map((cell) => `<td>${escapeHtml(cell)}</td>`).join("")}</tr>`,
          )
          .join("")
      : `<tr><td colspan="${Math.max(headers.length, 1)}" class="medousa-view-empty">No matching rows</td></tr>`;
  const csvPayload = encodeURIComponent(rowsToCsv(headers, rows));
  const editIndexAttr =
    viewIndex != null
      ? ` data-edit-view-index="${escapeAttr(String(viewIndex))}"`
      : "";
  const configureBtn =
    viewIndex != null
      ? `<button type="button" class="medousa-view-configure" data-edit-view-index="${escapeAttr(String(viewIndex))}">Configure</button>`
      : "";

  return `<div class="medousa-view" data-view-source="${escapeAttr(sourcePath)}"${editIndexAttr}>
  <div class="medousa-view-header">
    <span class="medousa-view-label">Query view</span>
    <div class="medousa-view-actions">
      <button type="button" class="medousa-view-copy-csv" data-copy-view-csv="${escapeAttr(csvPayload)}" data-view-csv="${escapeAttr(csvPayload)}">Copy CSV</button>
      ${configureBtn}
      <button type="button" class="medousa-view-edit-source" data-open-vault-note="${escapeAttr(sourcePath)}">Edit source</button>
    </div>
  </div>
  <div class="medousa-view-table-wrap">
    <table class="medousa-view-table">
      <thead><tr>${headerCells}</tr></thead>
      <tbody>${bodyRows}</tbody>
    </table>
  </div>
  <p class="medousa-view-meta">${rows.length} row${rows.length === 1 ? "" : "s"} · ${escapeHtml(sourceLabel)}</p>
</div>`;
}
