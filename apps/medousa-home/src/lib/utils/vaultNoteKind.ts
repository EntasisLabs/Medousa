import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";

/** Structured object-first layouts; wiki chrome stays secondary. */
export function isDataFirstKind(kind: VaultNoteKind): boolean {
  return (
    kind === "ledger" ||
    kind === "sheet" ||
    kind === "workbook" ||
    kind === "board" ||
    kind === "slides" ||
    kind === "draw"
  );
}

export function supportsLinksPanel(kind: VaultNoteKind): boolean {
  return kind !== "ledger" && kind !== "sheet" && kind !== "workbook";
}

export function supportsPreviewSplit(kind: VaultNoteKind): boolean {
  return kind !== "ledger" && kind !== "sheet" && kind !== "workbook" && kind !== "draw";
}
