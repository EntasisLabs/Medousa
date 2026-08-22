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

/**
 * Opaque H07 note_version for If-Match when present; digest content_hash only
 * for legacy residents that still advertise a digest token.
 */
export function vaultIfMatchToken(response: {
  note_version?: string | null;
  note: { content_hash: string };
}): string {
  const version = response.note_version?.trim();
  if (version) return version;
  return response.note.content_hash;
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
