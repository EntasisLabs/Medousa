/** Local vault UX preferences (localStorage). */

const STAMP_COMPLETION_KEY = "medousa-vault-stamp-completion";
const BUILD_WORD_WRAP_KEY = "medousa-vault-build-word-wrap";
const BUILD_LINE_NUMBERS_KEY = "medousa-vault-build-line-numbers";
const BUILD_AUTO_SAVE_KEY = "medousa-vault-build-auto-save";

function readBool(key: string, defaultValue: boolean): boolean {
  if (typeof localStorage === "undefined") return defaultValue;
  const raw = localStorage.getItem(key);
  if (raw === null) return defaultValue;
  return raw === "true";
}

function writeBool(key: string, enabled: boolean): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(key, enabled ? "true" : "false");
}

export function readVaultStampCompletionEnabled(): boolean {
  return readBool(STAMP_COMPLETION_KEY, false);
}

export function writeVaultStampCompletionEnabled(enabled: boolean): void {
  writeBool(STAMP_COMPLETION_KEY, enabled);
}

/** Build CodeMirror: wrap long lines (default on). */
export function readVaultBuildWordWrap(): boolean {
  return readBool(BUILD_WORD_WRAP_KEY, true);
}

export function writeVaultBuildWordWrap(enabled: boolean): void {
  writeBool(BUILD_WORD_WRAP_KEY, enabled);
}

/** Build CodeMirror: show line number gutter (default off). */
export function readVaultBuildLineNumbers(): boolean {
  return readBool(BUILD_LINE_NUMBERS_KEY, false);
}

export function writeVaultBuildLineNumbers(enabled: boolean): void {
  writeBool(BUILD_LINE_NUMBERS_KEY, enabled);
}

/** Autosave dirty notes on a timer (default on). */
export function readVaultBuildAutoSave(): boolean {
  return readBool(BUILD_AUTO_SAVE_KEY, true);
}

export function writeVaultBuildAutoSave(enabled: boolean): void {
  writeBool(BUILD_AUTO_SAVE_KEY, enabled);
}
