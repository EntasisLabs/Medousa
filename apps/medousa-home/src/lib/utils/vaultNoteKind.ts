import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";

/** Ledger, board — object-first layouts; wiki chrome stays secondary. */
export function isDataFirstKind(kind: VaultNoteKind): boolean {
  return kind === "ledger" || kind === "board";
}

export function supportsLinksPanel(kind: VaultNoteKind): boolean {
  return kind !== "ledger";
}

export function supportsPreviewSplit(kind: VaultNoteKind): boolean {
  return kind !== "ledger";
}
