/** CodeMirror helpers for vault markdown editing. */

import { tags as t } from "@lezer/highlight";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorSelection, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
} from "@codemirror/view";
import type { EditResult } from "$lib/utils/vaultMarkdownEdit";
import type { FindMatch } from "$lib/utils/vaultFindInNote";
import {
  placeSlashMenuAnchor,
  type SlashMenuAnchor,
} from "$lib/utils/slashMenuPlacement";

export function applyEditResult(view: EditorView, result: EditResult) {
  const old = view.state.doc.toString();
  const next = result.content;
  if (old === next) {
    view.dispatch({
      selection: EditorSelection.range(result.selectionStart, result.selectionEnd),
      scrollIntoView: true,
    });
    return;
  }

  let start = 0;
  const oldLen = old.length;
  const nextLen = next.length;
  while (start < oldLen && start < nextLen && old[start] === next[start]) {
    start += 1;
  }
  let oldEnd = oldLen;
  let nextEnd = nextLen;
  while (
    oldEnd > start &&
    nextEnd > start &&
    old[oldEnd - 1] === next[nextEnd - 1]
  ) {
    oldEnd -= 1;
    nextEnd -= 1;
  }

  view.dispatch({
    changes: { from: start, to: oldEnd, insert: next.slice(start, nextEnd) },
    selection: EditorSelection.range(result.selectionStart, result.selectionEnd),
    scrollIntoView: true,
  });
  view.focus();
}

export function revealFindMatchInView(
  view: EditorView,
  match: FindMatch | null,
) {
  if (!match) return;
  view.dispatch({
    selection: EditorSelection.range(match.start, match.end),
    scrollIntoView: true,
  });
}

const findMark = Decoration.mark({ class: "cm-vault-find-mark" });
const findMarkActive = Decoration.mark({ class: "cm-vault-find-mark-active" });

export function buildFindDecorations(
  matches: FindMatch[],
  activeIndex: number,
): DecorationSet {
  if (matches.length === 0) return Decoration.none;
  const safeIndex =
    ((activeIndex % matches.length) + matches.length) % matches.length;
  const ranges = matches.map((match, index) =>
    (index === safeIndex ? findMarkActive : findMark).range(match.start, match.end),
  );
  return Decoration.set(ranges, true);
}

/** External find-highlight decorations driven by vaultFind. */
export function vaultFindHighlightExtension(
  getHighlights: () => { matches: FindMatch[]; activeIndex: number },
): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        const { matches, activeIndex } = getHighlights();
        this.decorations = buildFindDecorations(matches, activeIndex);
        void view;
      }

      update() {
        const { matches, activeIndex } = getHighlights();
        this.decorations = buildFindDecorations(matches, activeIndex);
      }
    },
    {
      decorations: (value) => value.decorations,
    },
  );
}

/** Caret-aware slash menu placement inside an editor shell. */
export function getCodeMirrorCaretAnchor(
  view: EditorView,
  relativeTo?: HTMLElement | null,
): SlashMenuAnchor {
  const head = view.state.selection.main.head;
  const coords = view.coordsAtPos(head);
  const shell = relativeTo ?? view.dom;
  if (!coords) {
    const origin = shell.getBoundingClientRect();
    return placeSlashMenuAnchor(
      {
        top: origin.top + 40,
        bottom: origin.top + 58,
        left: origin.left + 16,
      },
      shell,
    );
  }
  return placeSlashMenuAnchor(
    { top: coords.top, bottom: coords.bottom, left: coords.left },
    shell,
  );
}

/** Dark-mode vault editor chrome (matches Grapheme / app surfaces). */
export const vaultEditorBaseTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      fontSize: "inherit",
      color: "rgb(var(--color-surface-100))",
      backgroundColor: "transparent",
    },
    ".cm-scroller": {
      overflow: "auto",
      fontFamily: "inherit",
      lineHeight: "inherit",
    },
    ".cm-content": {
      padding: "0",
      caretColor: "rgb(var(--color-primary-200))",
      color: "rgb(var(--color-surface-100))",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "rgb(var(--color-primary-200))",
      borderLeftWidth: "2px",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "rgb(var(--color-primary-100))",
    },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      backgroundColor: "rgb(var(--color-primary-500) / 0.28) !important",
    },
    ".cm-gutters": {
      display: "none",
    },
    ".cm-activeLine": {
      backgroundColor: "rgb(var(--color-surface-900) / 0.45)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "transparent",
    },
    "&.cm-focused": {
      outline: "none",
    },
    ".cm-placeholder": {
      color: "rgb(var(--color-surface-500))",
    },
    ".cm-vault-find-mark": {
      backgroundColor: "rgb(250 204 21 / 0.38)",
    },
    ".cm-vault-find-mark-active": {
      backgroundColor: "rgb(250 204 21 / 0.62)",
      boxShadow: "inset 0 -1.5px 0 0 rgb(234 179 8 / 0.85)",
    },
  },
  { dark: true },
);

/** Shown when Build line-numbers pref is on (overrides base gutters: none). */
export const vaultEditorLineNumbersTheme = EditorView.theme(
  {
    ".cm-gutters": {
      display: "flex",
      backgroundColor: "transparent",
      border: "none",
      color: "rgb(var(--shell-muted, var(--color-surface-500)))",
      minWidth: "2.1rem",
    },
    ".cm-lineNumbers .cm-gutterElement": {
      padding: "0 0.45rem 0 0.15rem",
      minWidth: "1.75rem",
    },
    ".cm-foldGutter": {
      display: "none",
    },
  },
  { dark: true },
);

/** Markdown highlighting tuned for dark surfaces (overrides basicSetup light defaults). */
export const vaultMarkdownHighlightStyle = HighlightStyle.define([
  { tag: t.heading1, color: "rgb(var(--color-primary-200))", fontWeight: "700" },
  { tag: t.heading2, color: "rgb(var(--color-primary-200))", fontWeight: "650" },
  { tag: t.heading3, color: "rgb(var(--color-primary-300))", fontWeight: "600" },
  { tag: t.heading4, color: "rgb(var(--color-primary-300))", fontWeight: "600" },
  { tag: t.heading5, color: "rgb(var(--color-surface-100))", fontWeight: "600" },
  { tag: t.heading6, color: "rgb(var(--color-surface-100))", fontWeight: "600" },
  { tag: t.strong, color: "rgb(var(--color-surface-50))", fontWeight: "700" },
  { tag: t.emphasis, color: "rgb(var(--color-surface-100))", fontStyle: "italic" },
  { tag: t.strikethrough, color: "rgb(var(--color-surface-400))", textDecoration: "line-through" },
  { tag: t.link, color: "rgb(var(--color-primary-300))" },
  { tag: t.url, color: "rgb(var(--color-secondary-300))" },
  { tag: t.monospace, color: "rgb(var(--color-warning-200))" },
  { tag: t.quote, color: "rgb(var(--color-surface-300))", fontStyle: "italic" },
  { tag: t.list, color: "rgb(var(--color-surface-200))" },
  { tag: t.meta, color: "rgb(var(--color-surface-400))" },
  { tag: t.processingInstruction, color: "rgb(var(--color-surface-400))" },
  { tag: t.contentSeparator, color: "rgb(var(--color-surface-500))" },
  { tag: t.comment, color: "rgb(var(--color-surface-500))", fontStyle: "italic" },
  { tag: t.atom, color: "rgb(var(--color-warning-200))" },
  { tag: t.bool, color: "rgb(var(--color-warning-200))" },
  { tag: t.literal, color: "rgb(var(--color-success-300))" },
  { tag: t.string, color: "rgb(var(--color-success-300))" },
]);

export const vaultMarkdownSyntax = syntaxHighlighting(vaultMarkdownHighlightStyle, {
  fallback: true,
});
