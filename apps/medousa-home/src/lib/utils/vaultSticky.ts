/** Shared localStorage handoff for the vault sticky-note pop-out. */
export const VAULT_STICKY_PATH_KEY = "medousa-home-vault-sticky-path";

/** Same key the vault store writes on openNote — tray/sticky fallback. */
export const VAULT_LAST_PATH_KEY = "medousa-home-last-note";

export function writeVaultStickyPath(path: string): void {
  const trimmed = path.trim();
  if (!trimmed || typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(VAULT_STICKY_PATH_KEY, trimmed);
  } catch {
    /* ignore quota / private mode */
  }
}

export function readVaultStickyPath(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(VAULT_STICKY_PATH_KEY);
    const trimmed = raw?.trim() ?? "";
    return trimmed || null;
  } catch {
    return null;
  }
}

export function clearVaultStickyPath(): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.removeItem(VAULT_STICKY_PATH_KEY);
  } catch {
    /* ignore */
  }
}

export function writeVaultLastPath(path: string): void {
  const trimmed = path.trim();
  if (!trimmed || typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(VAULT_LAST_PATH_KEY, trimmed);
  } catch {
    /* ignore */
  }
}

export function readVaultLastPath(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(VAULT_LAST_PATH_KEY);
    const trimmed = raw?.trim() ?? "";
    return trimmed || null;
  } catch {
    return null;
  }
}

/** Prefer floated sticky path; else last opened vault note. */
export function resolveVaultStickyOpenPath(): string | null {
  return readVaultStickyPath() ?? readVaultLastPath();
}
