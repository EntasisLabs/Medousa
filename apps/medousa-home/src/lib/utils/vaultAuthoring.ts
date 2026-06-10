import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";

export type VaultAuthoringMode = "write" | "source";

const WRITE_FIRST_KINDS = new Set<VaultNoteKind>(["daily", "note", "inbox"]);

export function isWriteFirstKind(kind: VaultNoteKind): boolean {
  return WRITE_FIRST_KINDS.has(kind);
}

export function defaultAuthoringMode(kind: VaultNoteKind): VaultAuthoringMode {
  return isWriteFirstKind(kind) ? "write" : "source";
}
