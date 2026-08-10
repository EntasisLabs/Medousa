/**
 * Thin shim: active syntax highlight style tracks the user preference.
 * Shell `--syn-*` mapping no longer drives Code / Grapheme editors.
 */
import type { HighlightStyle } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { readCodeEditorSyntaxTheme } from "$lib/config/codeEditorPreferences";
import {
  DEFAULT_CODE_SYNTAX_THEME,
  getCodeSyntaxTheme,
} from "$lib/syntax/codeSyntaxThemes";

/** HighlightStyle for the user’s current syntax theme preference. */
export function activeCodeSyntaxHighlightStyle(): HighlightStyle {
  return getCodeSyntaxTheme(readCodeEditorSyntaxTheme()).highlightStyle;
}

/** syntaxHighlighting() extension for the current preference. */
export function activeCodeSyntaxHighlighting(): Extension {
  return getCodeSyntaxTheme(readCodeEditorSyntaxTheme()).highlighting;
}

/**
 * Default Dark+ style snapshot for transitional imports.
 * Prefer `activeCodeSyntaxHighlightStyle()` so diffs track the user pack.
 */
export const medousaSyntaxHighlightStyle =
  getCodeSyntaxTheme(DEFAULT_CODE_SYNTAX_THEME).highlightStyle;

/**
 * Default Dark+ highlighting extension snapshot.
 * Prefer host-owned `buildCodeSyntaxThemeExtensions`.
 */
export const medousaSyntaxHighlighting =
  getCodeSyntaxTheme(DEFAULT_CODE_SYNTAX_THEME).highlighting;
