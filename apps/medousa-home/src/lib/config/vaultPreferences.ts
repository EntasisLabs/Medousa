/** Local vault UX preferences (localStorage). */

const STAMP_COMPLETION_KEY = "medousa-vault-stamp-completion";
const BUILD_WORD_WRAP_KEY = "medousa-vault-build-word-wrap";
const BUILD_LINE_NUMBERS_KEY = "medousa-vault-build-line-numbers";
const BUILD_AUTO_SAVE_KEY = "medousa-vault-build-auto-save";
const BUILD_SCROLL_SYNC_KEY = "medousa-vault-build-scroll-sync";
const READING_PALETTE_KEY = "medousa-vault-reading-palette";
const HIDE_LIVE_MARKDOWN_SYNTAX_KEY = "medousa-vault-hide-live-markdown-syntax";
const PAPER_WIDTH_KEY = "medousa-vault-paper-width";

export type VaultReadingPalette = "neutral" | "warm" | "cool" | "ink";

/** Live / Preview reading column width (paper view). */
export type VaultPaperWidth = "narrow" | "medium" | "wide" | "full";

export const VAULT_PAPER_WIDTHS: VaultPaperWidth[] = [
  "narrow",
  "medium",
  "wide",
  "full",
];

export function isVaultPaperWidth(value: string): value is VaultPaperWidth {
  return (VAULT_PAPER_WIDTHS as string[]).includes(value);
}

export const VAULT_READING_PALETTES: VaultReadingPalette[] = [
  "neutral",
  "warm",
  "cool",
  "ink",
];

export function isVaultReadingPalette(value: string): value is VaultReadingPalette {
  return (VAULT_READING_PALETTES as string[]).includes(value);
}

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

/** Build split: sync CodeMirror ↔ Preview scroll (default on). */
export function readVaultBuildScrollSync(): boolean {
  return readBool(BUILD_SCROLL_SYNC_KEY, true);
}

export function writeVaultBuildScrollSync(enabled: boolean): void {
  writeBool(BUILD_SCROLL_SYNC_KEY, enabled);
}

/** Live / preview reading palette (not shell colorTheme). */
export function readVaultReadingPalette(): VaultReadingPalette {
  if (typeof localStorage === "undefined") return "neutral";
  const raw = localStorage.getItem(READING_PALETTE_KEY);
  if (raw && isVaultReadingPalette(raw)) return raw;
  return "neutral";
}

export function writeVaultReadingPalette(palette: VaultReadingPalette): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(READING_PALETTE_KEY, palette);
}

export function cycleVaultReadingPalette(
  current: VaultReadingPalette,
): VaultReadingPalette {
  const index = VAULT_READING_PALETTES.indexOf(current);
  const next = VAULT_READING_PALETTES[(index + 1) % VAULT_READING_PALETTES.length];
  return next ?? "neutral";
}

/** Live: hide focused heading `#` / `##` widgets (WYSIWYG, no layout shift). */
export function readVaultHideLiveMarkdownSyntax(): boolean {
  return readBool(HIDE_LIVE_MARKDOWN_SYNTAX_KEY, false);
}

export function writeVaultHideLiveMarkdownSyntax(enabled: boolean): void {
  writeBool(HIDE_LIVE_MARKDOWN_SYNTAX_KEY, enabled);
}

export function readVaultPaperWidth(): VaultPaperWidth {
  if (typeof localStorage === "undefined") return "wide";
  const raw = localStorage.getItem(PAPER_WIDTH_KEY);
  if (raw && isVaultPaperWidth(raw)) return raw;
  return "wide";
}

export function writeVaultPaperWidth(width: VaultPaperWidth): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(PAPER_WIDTH_KEY, width);
}

export function cycleVaultPaperWidth(current: VaultPaperWidth): VaultPaperWidth {
  const index = VAULT_PAPER_WIDTHS.indexOf(current);
  return VAULT_PAPER_WIDTHS[(index + 1) % VAULT_PAPER_WIDTHS.length] ?? "wide";
}
