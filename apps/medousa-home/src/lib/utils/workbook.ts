/**
 * Vault workbooks — folder marker + sheet formulas frontmatter.
 * Formulas live in YAML only; GFM table cells stay literals/empties (overlay eval).
 */

import {
  normalizeKind,
  serializeFrontmatter,
  stripFrontmatter,
} from "$lib/utils/vaultFrontmatter";
import { findFirstPipeTable, type MarkdownTable } from "$lib/utils/markdownTable";

export const WORKBOOK_MARKER_FILE = "_workbook.md";
export const WORKBOOK_DOT_MARKER = ".medousa-workbook";

/** A1 cell address: A1, AA12, $B$3 (absolute markers stripped for storage). */
const A1_RE = /^\$?([A-Za-z]+)\$?([1-9][0-9]*)$/;

export interface WorkbookManifest {
  title: string;
  /** Sheet stem names (no .md), ordered. */
  sheets: string[];
}

export interface SheetFormulas {
  /** Normalized A1 → formula text including leading `=`. */
  formulas: Record<string, string>;
}

export function isWorkbookMarkerPath(path: string): boolean {
  const base = path.split("/").pop() ?? path;
  return base === WORKBOOK_MARKER_FILE || base === WORKBOOK_DOT_MARKER;
}

/** Parent folder of `_workbook.md` / `.medousa-workbook`. */
export function workbookFolderFromMarkerPath(path: string): string | null {
  if (!isWorkbookMarkerPath(path)) return null;
  const idx = path.lastIndexOf("/");
  return idx >= 0 ? path.slice(0, idx) : "";
}

export function normalizeA1(addr: string): string | null {
  const m = A1_RE.exec(addr.trim());
  if (!m) return null;
  return `${m[1]!.toUpperCase()}${m[2]!}`;
}

export function isValidA1(addr: string): boolean {
  return normalizeA1(addr) !== null;
}

/** Column letters → 0-based index (A=0). */
export function colLettersToIndex(letters: string): number {
  let n = 0;
  for (const ch of letters.toUpperCase()) {
    n = n * 26 + (ch.charCodeAt(0) - 64);
  }
  return n - 1;
}

export function colIndexToLetters(index: number): string {
  let n = index + 1;
  let out = "";
  while (n > 0) {
    const rem = (n - 1) % 26;
    out = String.fromCharCode(65 + rem) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}

export function a1ToRowCol(addr: string): { row: number; col: number } | null {
  const norm = normalizeA1(addr);
  if (!norm) return null;
  const m = A1_RE.exec(norm);
  if (!m) return null;
  return { col: colLettersToIndex(m[1]!), row: Number(m[2]!) - 1 };
}

export function rowColToA1(row: number, col: number): string {
  return `${colIndexToLetters(col)}${row + 1}`;
}

function readYamlList(frontmatter: string, key: string): string[] {
  const lines = frontmatter.replace(/\r\n/g, "\n").split("\n");
  const prefix = `${key}:`;
  for (let i = 0; i < lines.length; i++) {
    const trimmed = (lines[i] ?? "").trim();
    if (!trimmed.startsWith(prefix)) continue;
    const inline = trimmed.slice(prefix.length).trim();
    if (inline.startsWith("[") && inline.endsWith("]")) {
      const inner = inline.slice(1, -1).trim();
      if (!inner) return [];
      return inner
        .split(",")
        .map((s) => s.trim().replace(/^['"]|['"]$/g, ""))
        .filter(Boolean);
    }
    const out: string[] = [];
    for (let k = i + 1; k < lines.length; k++) {
      const raw = lines[k] ?? "";
      const item = raw.match(/^\s*-\s+(.+)$/);
      if (item) {
        out.push(item[1]!.trim().replace(/^['"]|['"]$/g, ""));
        continue;
      }
      if (raw.trim() && !/^\s/.test(raw)) break;
    }
    return out;
  }
  return [];
}

function readYamlScalar(frontmatter: string, key: string): string {
  const prefix = `${key}:`;
  for (const raw of frontmatter.split("\n")) {
    const trimmed = raw.trim();
    if (!trimmed.startsWith(prefix)) continue;
    return trimmed.slice(prefix.length).trim().replace(/^['"]|['"]$/g, "");
  }
  return "";
}

/** Parse `_workbook.md` body into a manifest. */
export function parseWorkbookManifest(markdown: string): WorkbookManifest | null {
  const { frontmatter, content } = stripFrontmatter(markdown);
  const fm = frontmatter ?? "";
  const kind = normalizeKind(readYamlScalar(fm, "kind") || "workbook");
  if (kind !== "workbook" && !fm.includes("kind: workbook")) {
    // Allow marker without kind if sheets: present
    if (!fm.includes("sheets:") && !content.includes("sheets:")) {
      // soft: still accept if title + sheets in frontmatter
    }
  }
  const title =
    readYamlScalar(fm, "title") ||
    readYamlScalar(content, "title") ||
    "Workbook";
  const sheets = readYamlList(fm, "sheets");
  if (sheets.length === 0) {
    // try content if someone put sheets outside frontmatter
    const fromBody = readYamlList(
      content
        .replace(/^---\n/, "")
        .split("\n")
        .filter((l) => !l.startsWith("#"))
        .join("\n"),
      "sheets",
    );
    if (fromBody.length === 0) return null;
    return { title, sheets: fromBody.map(sheetStem) };
  }
  return { title, sheets: sheets.map(sheetStem) };
}

export function sheetStem(name: string): string {
  const trimmed = name.trim().replace(/\.md$/i, "");
  return trimmed;
}

export function serializeWorkbookManifest(manifest: WorkbookManifest): string {
  const lines = [
    "kind: workbook",
    `title: ${manifest.title.trim() || "Workbook"}`,
    "sheets:",
    ...manifest.sheets.map((s) => `  - ${sheetStem(s)}`),
  ];
  return serializeFrontmatter(lines.join("\n"), "");
}

export function createEmptyWorkbookMarker(
  title = "Untitled workbook",
  sheets: string[] = ["Sheet1"],
): string {
  return serializeWorkbookManifest({ title, sheets });
}

/** Parse `formulas:` map from sheet note frontmatter. */
export function parseSheetFormulas(markdown: string): SheetFormulas {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return { formulas: {} };
  return { formulas: parseFormulasYamlMap(frontmatter) };
}

export function parseFormulasYamlMap(frontmatter: string): Record<string, string> {
  const lines = frontmatter.replace(/\r\n/g, "\n").split("\n");
  const out: Record<string, string> = {};
  let inFormulas = false;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i] ?? "";
    const trimmed = raw.trim();
    if (!inFormulas) {
      if (trimmed === "formulas:" || trimmed.startsWith("formulas:")) {
        const inline = trimmed.slice("formulas:".length).trim();
        if (inline.startsWith("{") && inline.endsWith("}")) {
          // minimal inline object — rarely used
          return out;
        }
        inFormulas = true;
      }
      continue;
    }
    if (trimmed && !/^\s/.test(raw) && !trimmed.startsWith("#")) {
      break;
    }
    const m = raw.match(/^\s+([A-Za-z]+[0-9]+)\s*:\s*(.+)$/);
    if (!m) continue;
    const addr = normalizeA1(m[1]!);
    if (!addr) continue;
    let formula = m[2]!.trim().replace(/^['"]|['"]$/g, "");
    if (!formula.startsWith("=")) formula = `=${formula}`;
    out[addr] = formula;
  }
  return out;
}

/** Replace or insert the `formulas:` block; preserves other YAML. */
export function setSheetFormulasYaml(
  frontmatter: string | null,
  formulas: Record<string, string>,
): string {
  const lines = (frontmatter ?? "").replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let i = 0;
  let wrote = false;
  while (i < lines.length) {
    const raw = lines[i] ?? "";
    const trimmed = raw.trim();
    if (trimmed === "formulas:" || trimmed.startsWith("formulas:")) {
      // skip existing block
      i += 1;
      while (i < lines.length) {
        const next = lines[i] ?? "";
        if (next.trim() && !/^\s/.test(next)) break;
        i += 1;
      }
      if (Object.keys(formulas).length > 0) {
        out.push(...serializeFormulasBlock(formulas));
        wrote = true;
      }
      continue;
    }
    out.push(raw);
    i += 1;
  }
  if (!wrote && Object.keys(formulas).length > 0) {
    while (out.length > 0 && !(out[out.length - 1] ?? "").trim()) out.pop();
    out.push(...serializeFormulasBlock(formulas));
  }
  return out.join("\n").replace(/^\n+/, "").replace(/\n+$/, "");
}

function serializeFormulasBlock(formulas: Record<string, string>): string[] {
  const keys = Object.keys(formulas).sort((a, b) => {
    const ra = a1ToRowCol(a);
    const rb = a1ToRowCol(b);
    if (!ra || !rb) return a.localeCompare(b);
    return ra.row - rb.row || ra.col - rb.col;
  });
  const block = ["formulas:"];
  for (const key of keys) {
    const formula = formulas[key]!;
    const text = formula.startsWith("=") ? formula : `=${formula}`;
    block.push(`  ${key}: "${text.replace(/"/g, '\\"')}"`);
  }
  return block;
}

export function upsertSheetFormulas(
  markdown: string,
  formulas: Record<string, string>,
): string {
  const { frontmatter, content } = stripFrontmatter(markdown);
  const nextFm = setSheetFormulasYaml(frontmatter, formulas);
  // Ensure kind: sheet when formulas present
  let fm = nextFm;
  if (!/^\s*kind:\s*/m.test(fm)) {
    fm = `kind: sheet\n${fm}`.replace(/\n+$/, "");
  }
  return serializeFrontmatter(fm, content);
}

export function noteIsWorkbookManifest(markdown: string): boolean {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return false;
  const kind = normalizeKind(readYamlScalar(frontmatter, "kind"));
  return kind === "workbook";
}

export function noteIsSheet(markdown: string): boolean {
  const { frontmatter } = stripFrontmatter(markdown);
  if (!frontmatter) return false;
  const kind = normalizeKind(readYamlScalar(frontmatter, "kind"));
  return kind === "sheet";
}

/** Table grid for HyperFormula — header row + data rows as strings. */
export function tableGridFromMarkdown(markdown: string): {
  headers: string[];
  rows: string[][];
  table: MarkdownTable | null;
} {
  const { content } = stripFrontmatter(markdown);
  const table = findFirstPipeTable(content);
  if (!table) return { headers: [], rows: [], table: null };
  return { headers: table.headers, rows: table.rows, table };
}
