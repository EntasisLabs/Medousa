import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";

/** Ledger, sheet, board, slides — object-first layouts; wiki chrome stays secondary. */
export function isDataFirstKind(kind: VaultNoteKind): boolean {
  return (
    kind === "ledger" ||
    kind === "sheet" ||
    kind === "workbook" ||
    kind === "board" ||
    kind === "slides"
  );
}

export function supportsLinksPanel(kind: VaultNoteKind): boolean {
  return kind !== "ledger" && kind !== "sheet";
}

export function supportsPreviewSplit(kind: VaultNoteKind): boolean {
  return kind !== "ledger" && kind !== "sheet";
}
