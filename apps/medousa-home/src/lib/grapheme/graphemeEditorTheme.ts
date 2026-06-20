import { tags as t } from "@lezer/highlight";
import { HighlightStyle, LanguageSupport, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { graphemeLanguage } from "$lib/grapheme/graphemeLanguage";

export const graphemeEditorTheme = EditorView.theme(
  {
    "&": {
      color: "rgb(var(--color-surface-100))",
      backgroundColor: "rgb(var(--color-surface-950))",
      height: "100%",
    },
    ".cm-content": {
      fontFamily:
        'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: "13px",
      lineHeight: "1.6",
      caretColor: "rgb(var(--color-primary-300))",
      padding: "12px 0",
    },
    ".cm-gutters": {
      backgroundColor: "rgb(var(--color-surface-950))",
      color: "rgb(var(--color-surface-500))",
      borderRight: "1px solid rgb(var(--color-surface-600) / 0.45)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgb(var(--color-surface-900))",
    },
    ".cm-activeLine": {
      backgroundColor: "rgb(var(--color-surface-900) / 0.55)",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgb(var(--color-primary-500) / 0.22) !important",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "rgb(var(--color-primary-300))",
    },
    ".cm-scroller": {
      overflow: "auto",
    },
  },
  { dark: true },
);

export const graphemeHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: "rgb(var(--color-secondary-300))" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "rgb(var(--color-primary-300))" },
  { tag: [t.typeName, t.namespace], color: "rgb(var(--color-warning-300))" },
  { tag: t.string, color: "rgb(var(--color-success-300))" },
  { tag: t.number, color: "rgb(var(--color-warning-200))" },
  { tag: t.operator, color: "rgb(var(--color-surface-300))" },
  { tag: t.variableName, color: "rgb(var(--color-surface-100))" },
  { tag: t.special(t.variableName), color: "rgb(var(--color-primary-200))" },
  { tag: t.comment, color: "rgb(var(--color-surface-500))", fontStyle: "italic" },
]);

export const graphemeSyntax = syntaxHighlighting(graphemeHighlightStyle);

export const graphemeLanguageSupport = new LanguageSupport(graphemeLanguage, [
  graphemeSyntax,
]);
