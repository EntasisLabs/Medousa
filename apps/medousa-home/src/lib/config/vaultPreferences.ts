/** Local vault UX preferences (localStorage). */

const STAMP_COMPLETION_KEY = "medousa-vault-stamp-completion";

export function readVaultStampCompletionEnabled(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(STAMP_COMPLETION_KEY) === "true";
}

export function writeVaultStampCompletionEnabled(enabled: boolean): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STAMP_COMPLETION_KEY, enabled ? "true" : "false");
}
