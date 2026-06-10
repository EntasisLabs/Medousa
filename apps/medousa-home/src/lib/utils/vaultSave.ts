/** M7d.2 — vault save helpers (conflict detection, autosave timing). */

export const VAULT_AUTOSAVE_MS = 4500;
export const VAULT_SAVED_WHISPER_MS = 2200;
/** Debounce tree refresh after save/SSE so the sidebar does not flicker. */
export const VAULT_NOTES_REFRESH_MS = 800;
/** Ignore operator vault SSE echo shortly after our own save. */
export const VAULT_SAVE_ECHO_MS = 4000;

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
