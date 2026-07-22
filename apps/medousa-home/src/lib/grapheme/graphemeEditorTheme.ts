import { LanguageSupport } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { graphemeLanguage } from "$lib/grapheme/graphemeLanguage";
import {
  medousaSyntaxHighlightStyle,
  medousaSyntaxHighlighting,
} from "$lib/syntax/codemirrorSyntaxTheme";

const tooltipShell = {
  border: "1px solid rgb(var(--color-surface-600) / 0.4)",
  backgroundColor: "rgb(var(--color-surface-900) / 0.98)",
  color: "rgb(var(--color-surface-100))",
  borderRadius: "8px",
  boxShadow: "0 12px 28px rgb(0 0 0 / 0.38), 0 0 0 1px rgb(0 0 0 / 0.14)",
} as const;

export const graphemeEditorTheme = EditorView.theme(
  {
    "&": {
      color: "rgb(var(--syn-fg))",
      backgroundColor: "rgb(var(--color-surface-950))",
      height: "100%",
    },
    ".cm-content": {
      fontFamily:
        'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: "13px",
      lineHeight: "1.45",
      caretColor: "rgb(var(--color-primary-300))",
      padding: "8px 0",
    },
    ".cm-gutters": {
      backgroundColor: "rgb(var(--color-surface-950))",
      color: "rgb(var(--color-surface-500))",
      borderRight: "1px solid rgb(var(--color-surface-600) / 0.4)",
      minWidth: "2.75rem",
    },
    ".cm-gutterElement": {
      padding: "0 0.5rem 0 0.65rem",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgb(var(--color-surface-900))",
      color: "rgb(var(--color-surface-300))",
    },
    ".cm-activeLine": {
      backgroundColor: "rgb(var(--color-surface-900) / 0.45)",
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

    /* Tooltips / completions / hover — Medousa, not stock CM */
    ".cm-tooltip": {
      ...tooltipShell,
      fontFamily:
        'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: "12px",
    },
    ".cm-tooltip.cm-tooltip-autocomplete": {
      ...tooltipShell,
      overflow: "hidden",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul": {
      fontFamily: "inherit",
      maxHeight: "14em",
      minWidth: "18rem",
      padding: "4px 0",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
      padding: "4px 10px",
      lineHeight: "1.35",
      borderRadius: "0",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
      backgroundColor: "rgb(var(--color-primary-500) / 0.22)",
      color: "rgb(var(--color-surface-50))",
    },
    ".cm-completionLabel": {
      color: "rgb(var(--color-surface-100))",
      fontStyle: "normal",
    },
    ".cm-completionMatchedText": {
      color: "rgb(var(--color-primary-200))",
      textDecoration: "none",
      fontWeight: "600",
    },
    ".cm-completionDetail": {
      marginLeft: "0.65em",
      color: "rgb(var(--color-surface-500))",
      fontStyle: "normal",
      fontSize: "11px",
    },
    ".cm-completionInfo, .cm-tooltip.cm-completionInfo": {
      ...tooltipShell,
      padding: "8px 10px",
      maxWidth: "22rem",
      lineHeight: "1.4",
      color: "rgb(var(--color-surface-200))",
      fontStyle: "normal",
    },
    ".cm-lsp-documentation, .cm-lsp-hover-tooltip": {
      padding: "8px 10px",
      lineHeight: "1.45",
      maxWidth: "26rem",
      whiteSpace: "pre-wrap",
      fontStyle: "normal",
      color: "rgb(var(--color-surface-200))",
    },
    ".cm-lsp-hover-tooltip.grapheme-hover": {
      padding: "9px 11px",
      maxWidth: "28rem",
      display: "flex",
      flexDirection: "column",
      gap: "6px",
    },
    ".grapheme-hover-title": {
      color: "rgb(var(--color-surface-50))",
      fontWeight: "600",
      fontSize: "12px",
      letterSpacing: "-0.01em",
      fontStyle: "normal",
    },
    ".grapheme-hover-blurb": {
      color: "rgb(var(--color-surface-400))",
      fontSize: "11px",
      lineHeight: "1.4",
      fontStyle: "normal",
    },
    ".grapheme-hover-row": {
      display: "grid",
      gridTemplateColumns: "4.25rem minmax(0, 1fr)",
      gap: "8px",
      alignItems: "start",
      fontSize: "11px",
      lineHeight: "1.4",
    },
    ".grapheme-hover-label": {
      color: "rgb(var(--color-surface-500))",
      fontWeight: "500",
      fontStyle: "normal",
      textTransform: "lowercase",
    },
    ".grapheme-hover-value": {
      color: "rgb(var(--color-surface-200))",
      fontStyle: "normal",
      wordBreak: "break-word",
    },
    ".cm-lsp-signature-tooltip": {
      ...tooltipShell,
      padding: "8px 10px",
    },
    ".cm-lsp-signature": {
      color: "rgb(var(--color-surface-100))",
      fontStyle: "normal",
    },
    ".cm-lsp-active-parameter": {
      color: "rgb(var(--color-primary-200))",
      fontWeight: "600",
      textDecoration: "underline",
      textUnderlineOffset: "2px",
    },

    /* Quiet type markers — no emoji */
    ".cm-completionIcon": {
      opacity: "1",
      width: "1.1em",
      paddingRight: "0.55em",
      fontSize: "10px",
      fontWeight: "700",
      fontStyle: "normal",
      color: "rgb(var(--color-surface-500))",
    },
    ".cm-completionIcon-keyword:after": {
      content: "'K'",
      color: "rgb(var(--color-secondary-300))",
    },
    ".cm-completionIcon-function:after, .cm-completionIcon-method:after": {
      content: "'f'",
      color: "rgb(var(--color-primary-300))",
    },
    ".cm-completionIcon-variable:after": {
      content: "'v'",
      color: "rgb(var(--color-surface-300))",
    },
    ".cm-completionIcon-type:after, .cm-completionIcon-class:after": {
      content: "'T'",
      color: "rgb(var(--color-warning-300))",
    },
    ".cm-completionIcon-property:after": {
      content: "'p'",
      color: "rgb(var(--color-surface-400))",
    },
    ".cm-completionIcon-namespace:after": {
      content: "'N'",
      color: "rgb(var(--color-warning-200))",
    },
    ".cm-completionIcon-constant:after": {
      content: "'C'",
      color: "rgb(var(--color-success-300))",
    },
    ".cm-completionIcon-text:after": {
      content: "'·'",
      color: "rgb(var(--color-surface-500))",
    },
  },
  { dark: true },
);

export const graphemeHighlightStyle = medousaSyntaxHighlightStyle;
export const graphemeSyntax = medousaSyntaxHighlighting;

export const graphemeLanguageSupport = new LanguageSupport(graphemeLanguage, [
  graphemeSyntax,
]);
