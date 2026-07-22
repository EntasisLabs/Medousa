/**
 * Object-first vault surfaces (sheet / workbook / slides / board / ledger):
 * ensure body + frontmatter so Live TipTap is not left empty or kind-less.
 */

import {
  normalizeKind,
  parseFrontmatterKindValue,
  resolveKind,
  setFrontmatterKind,
  stripFrontmatter,
  type VaultNoteKind,
} from "$lib/utils/vaultFrontmatter";
import { findLedgerTable, serializePipeTable } from "$lib/utils/markdownTable";
import {
  createEmptyWorkbookMarker,
  parseWorkbookManifest,
} from "$lib/utils/workbook";
import { noteHasKanbanBoard } from "$lib/utils/markdownKanban";
import { noteHasSlidesDeck } from "$lib/utils/markdownSlides";

const DEFAULT_SHEET_HEADERS = ["A", "B", "C", "D"];
const DEFAULT_LEDGER_HEADERS = ["Date", "Payee", "Amount", "Category"];

/** Resolve kind from frontmatter when present, else path inference. */
export function kindFromNoteContent(
  path: string | null | undefined,
  content: string,
): VaultNoteKind {
  const { frontmatter } = stripFrontmatter(content);
  const fmKind = parseFrontmatterKindValue(frontmatter).trim();
  return resolveKind(path ?? "", fmKind || undefined);
}

function appendPipeTable(content: string, headers: string[]): string {
  const table = serializePipeTable(headers, [headers.map(() => "")]);
  const trimmed = content.replace(/\s*$/, "");
  if (!trimmed) return `${table}\n`;
  return `${trimmed}\n\n${table}\n`;
}

/** Ensure sheet/ledger notes have a GFM table LedgerTableEditor can mount. */
export function ensureSheetOrLedgerTable(
  content: string,
  kind: "sheet" | "ledger",
): string {
  let next = setFrontmatterKind(content, kind);
  if (findLedgerTable(next)) return next;
  const headers = kind === "ledger" ? DEFAULT_LEDGER_HEADERS : DEFAULT_SHEET_HEADERS;
  return appendPipeTable(next, headers);
}

/**
 * After a kind pill change (or cold open), make sure the note body can host
 * the object editor instead of falling through to empty Live TipTap.
 */
export function ensureDataFirstSurface(
  kind: VaultNoteKind,
  content: string,
  title = "Untitled",
): string {
  const nextKind = normalizeKind(kind);
  switch (nextKind) {
    case "sheet":
      return ensureSheetOrLedgerTable(content, "sheet");
    case "ledger":
      return ensureSheetOrLedgerTable(content, "ledger");
    case "workbook": {
      const existing = parseWorkbookManifest(content);
      if (existing) return setFrontmatterKind(content, "workbook");
      return createEmptyWorkbookMarker(title.trim() || "Untitled workbook");
    }
    case "slides": {
      const withKind = setFrontmatterKind(content, "slides");
      // noteHasSlidesDeck is true once kind is slides — deck editor synthesizes body.
      return withKind;
    }
    case "board": {
      const withKind = setFrontmatterKind(content, "board");
      // Board editor defaults columns when body has no ## sections.
      return withKind;
    }
    default:
      return setFrontmatterKind(content, nextKind);
  }
}

/** Whether content already has the surface needed for this data-first kind. */
export function dataFirstSurfaceReady(
  kind: VaultNoteKind,
  content: string,
): boolean {
  switch (normalizeKind(kind)) {
    case "sheet":
    case "ledger":
      return Boolean(findLedgerTable(content));
    case "workbook":
      return parseWorkbookManifest(content) != null;
    case "slides":
      return noteHasSlidesDeck(content);
    case "board":
      return noteHasKanbanBoard(content);
    default:
      return true;
  }
}
