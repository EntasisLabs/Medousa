/** Scripts / code editor UX preferences (localStorage). */

import {
  DEFAULT_CODE_SYNTAX_THEME,
  resolveCodeSyntaxTheme,
  type CodeSyntaxThemeId,
} from "$lib/syntax/codeSyntaxThemes";

const WORD_WRAP_KEY = "medousa-code-editor-word-wrap";
const TAB_SIZE_KEY = "medousa-code-editor-tab-size";
const LINE_NUMBERS_KEY = "medousa-code-editor-line-numbers";
const FONT_SIZE_KEY = "medousa-code-editor-font-size";
const SYNTAX_THEME_KEY = "medousa-code-editor-syntax-theme";
const OUTLINE_OPEN_KEY = "medousa-code-editor-outline-open";
const PROBLEMS_OPEN_KEY = "medousa-code-editor-problems-open";

export type CodeEditorFontSize = 12 | 13 | 14 | 15 | 16;
export type { CodeSyntaxThemeId };

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

export function readCodeEditorWordWrap(): boolean {
  return readBool(WORD_WRAP_KEY, true);
}

export function writeCodeEditorWordWrap(enabled: boolean): void {
  writeBool(WORD_WRAP_KEY, enabled);
}

export function readCodeEditorLineNumbers(): boolean {
  return readBool(LINE_NUMBERS_KEY, true);
}

export function writeCodeEditorLineNumbers(enabled: boolean): void {
  writeBool(LINE_NUMBERS_KEY, enabled);
}

export function readCodeEditorFontSize(): CodeEditorFontSize {
  if (typeof localStorage === "undefined") return 13;
  const raw = localStorage.getItem(FONT_SIZE_KEY);
  const n = raw ? Number.parseInt(raw, 10) : 13;
  return n === 12 || n === 14 || n === 15 || n === 16 ? n : 13;
}

export function writeCodeEditorFontSize(size: number): void {
  if (typeof localStorage === "undefined") return;
  const next: CodeEditorFontSize =
    size === 12 || size === 14 || size === 15 || size === 16 ? size : 13;
  localStorage.setItem(FONT_SIZE_KEY, String(next));
}

export function readCodeEditorSyntaxTheme(): CodeSyntaxThemeId {
  if (typeof localStorage === "undefined") return DEFAULT_CODE_SYNTAX_THEME;
  return resolveCodeSyntaxTheme(localStorage.getItem(SYNTAX_THEME_KEY));
}

export function writeCodeEditorSyntaxTheme(id: string): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(SYNTAX_THEME_KEY, resolveCodeSyntaxTheme(id));
}

export function readCodeEditorTabSize(): number {
  if (typeof localStorage === "undefined") return 2;
  const raw = localStorage.getItem(TAB_SIZE_KEY);
  const n = raw ? Number.parseInt(raw, 10) : 2;
  return n === 4 || n === 8 ? n : 2;
}

export function hasCodeEditorTabSizePreference(): boolean {
  return typeof localStorage !== "undefined" && localStorage.getItem(TAB_SIZE_KEY) !== null;
}

export function writeCodeEditorTabSize(size: number): void {
  if (typeof localStorage === "undefined") return;
  const next = size === 4 || size === 8 ? size : 2;
  localStorage.setItem(TAB_SIZE_KEY, String(next));
}

export function readCodeEditorOutlineOpen(): boolean {
  return readBool(OUTLINE_OPEN_KEY, false);
}

export function writeCodeEditorOutlineOpen(open: boolean): void {
  writeBool(OUTLINE_OPEN_KEY, open);
}

export function readCodeEditorProblemsOpen(): boolean {
  return readBool(PROBLEMS_OPEN_KEY, true);
}

export function writeCodeEditorProblemsOpen(open: boolean): void {
  writeBool(PROBLEMS_OPEN_KEY, open);
}
