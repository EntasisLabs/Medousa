/** Ledger column enrichment: `{{Label key:value}}` (pipe-safe, no inner `|`). */

import {
  MARKDOWN_COLOR_IDS,
  type MarkdownColorId,
} from "$lib/utils/vaultMarkdownColors";

export type LedgerColumnType = "text" | "date" | "currency" | "number";
export type LedgerColumnAlign = "left" | "right" | "center";

export interface LedgerColumnMeta {
  width?: string;
  type?: LedgerColumnType;
  align?: LedgerColumnAlign;
  color?: MarkdownColorId;
}

export interface LedgerColumn {
  label: string;
  meta: LedgerColumnMeta;
}

const COLUMN_TYPES = new Set<LedgerColumnType>([
  "text",
  "date",
  "currency",
  "number",
]);
const COLUMN_ALIGNS = new Set<LedgerColumnAlign>(["left", "right", "center"]);
const COLOR_IDS = new Set<string>(MARKDOWN_COLOR_IDS);

const WIDTH_RE = /^(\d+(?:\.\d+)?)(px|rem|em|ch|%)$/;
const ENRICHED_RE = /^\{\{\s*([\s\S]*?)\s*\}\}$/;

function isMarkdownColorId(value: string): value is MarkdownColorId {
  return COLOR_IDS.has(value);
}

export function isValidLedgerWidth(value: string): boolean {
  return WIDTH_RE.test(value.trim());
}

function parseMetaEntries(raw: string): LedgerColumnMeta {
  const meta: LedgerColumnMeta = {};
  const trimmed = raw.trim();
  if (!trimmed) return meta;

  const pairs = trimmed.match(/[a-zA-Z][\w]*:[^\s]+/g) ?? [];
  for (const pair of pairs) {
    const colon = pair.indexOf(":");
    if (colon === -1) continue;
    const key = pair.slice(0, colon).trim().toLowerCase();
    const value = pair.slice(colon + 1).trim();
    if (!value) continue;

    if (key === "width" && isValidLedgerWidth(value)) {
      meta.width = value;
      continue;
    }
    if (key === "type" && COLUMN_TYPES.has(value as LedgerColumnType)) {
      meta.type = value as LedgerColumnType;
      continue;
    }
    if (key === "align" && COLUMN_ALIGNS.has(value as LedgerColumnAlign)) {
      meta.align = value as LedgerColumnAlign;
      continue;
    }
    if (key === "color" && isMarkdownColorId(value)) {
      meta.color = value;
    }
  }
  return meta;
}

/** Parse a single header cell into label + meta. Plain headers stay plain. */
export function parseColumnHeader(raw: string): LedgerColumn {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { label: "", meta: {} };
  }

  const enriched = trimmed.match(ENRICHED_RE);
  const inner = enriched ? enriched[1].trim() : trimmed;

  const kvStart = inner.search(/\s+[a-zA-Z][\w]*:/);
  let labelPart: string;
  let metaPart: string;
  if (kvStart === -1) {
    // Entire string may be a single label:key mistake — treat as label unless it's only KVs.
    const onlyKv = /^[a-zA-Z][\w]*:[^\s]+(?:\s+[a-zA-Z][\w]*:[^\s]+)*$/.test(inner);
    if (onlyKv && !inner.toLowerCase().startsWith("label:")) {
      return { label: "Column", meta: parseMetaEntries(inner) };
    }
    labelPart = inner;
    metaPart = "";
  } else {
    labelPart = inner.slice(0, kvStart).trim();
    metaPart = inner.slice(kvStart).trim();
  }

  if (labelPart.toLowerCase().startsWith("label:")) {
    labelPart = labelPart.slice("label:".length).trim();
  }

  return {
    label: labelPart || "Column",
    meta: parseMetaEntries(metaPart),
  };
}

export function serializeColumnHeader(column: LedgerColumn): string {
  const label = column.label.trim() || "Column";
  const parts: string[] = [label];
  const { meta } = column;
  if (meta.width && isValidLedgerWidth(meta.width)) parts.push(`width:${meta.width}`);
  if (meta.type) parts.push(`type:${meta.type}`);
  if (meta.align) parts.push(`align:${meta.align}`);
  if (meta.color) parts.push(`color:${meta.color}`);
  if (parts.length === 1) return label;
  return `{{${parts.join(" ")}}}`;
}

export function parseLedgerColumns(headers: string[]): LedgerColumn[] {
  return headers.map((header) => parseColumnHeader(header));
}

export function serializeLedgerColumns(columns: LedgerColumn[]): string[] {
  return columns.map((column) => serializeColumnHeader(column));
}

export function columnDisplayLabel(rawHeader: string): string {
  return parseColumnHeader(rawHeader).label;
}

export function mergeColumnMeta(
  column: LedgerColumn,
  patch: Partial<LedgerColumnMeta>,
): LedgerColumn {
  const nextMeta: LedgerColumnMeta = { ...column.meta, ...patch };
  for (const key of Object.keys(patch) as (keyof LedgerColumnMeta)[]) {
    if (patch[key] === undefined || patch[key] === "") {
      delete nextMeta[key];
    }
  }
  return { label: column.label, meta: nextMeta };
}

/** Effective alignment for a column (meta wins, then type, then name heuristics). */
export function resolveColumnAlign(
  column: LedgerColumn,
  colIndex: number,
): LedgerColumnAlign {
  if (column.meta.align) return column.meta.align;
  if (column.meta.type === "currency" || column.meta.type === "number") {
    return "right";
  }
  if (column.meta.type === "date") return "left";
  const lower = column.label.toLowerCase();
  if (lower.includes("amount") || lower.includes("total") || colIndex === 2) {
    return "right";
  }
  return "left";
}

export function resolveColumnType(
  column: LedgerColumn,
  colIndex: number,
): LedgerColumnType {
  if (column.meta.type) return column.meta.type;
  const lower = column.label.toLowerCase();
  if (lower.includes("amount") || lower.includes("total") || colIndex === 2) {
    return "currency";
  }
  if (lower.includes("date") || colIndex === 0) return "date";
  return "text";
}
