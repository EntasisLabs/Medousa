import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";

const WRITE_FIRST_KINDS = new Set<VaultNoteKind>(["daily", "note", "inbox"]);

/** Kinds that open in edit mode by default (journal-style notes). */
export function isWriteFirstKind(kind: VaultNoteKind): boolean {
  return WRITE_FIRST_KINDS.has(kind);
}
