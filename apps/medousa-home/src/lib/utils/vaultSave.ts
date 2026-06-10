/** M7d.2 — vault save helpers (conflict detection, autosave timing). */

export const VAULT_AUTOSAVE_MS = 1500;
export const VAULT_SAVED_WHISPER_MS = 2200;

export function isVaultConflictError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return (
    message.includes("412") ||
    message.includes("content_hash mismatch") ||
    message.includes("If-Match")
  );
}

export type VaultSaveStatus =
  | "idle"
  | "unsaved"
  | "saving"
  | "saved"
  | "conflict";

export function saveStatusLabel(status: VaultSaveStatus): string | null {
  switch (status) {
    case "unsaved":
      return "Unsaved";
    case "saving":
      return "Saving…";
    case "saved":
      return "Saved";
    case "conflict":
      return "Conflict";
    default:
      return null;
  }
}
