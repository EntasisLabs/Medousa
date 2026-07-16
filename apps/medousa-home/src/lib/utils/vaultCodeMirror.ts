/** CodeMirror helpers for vault markdown editing. */

import { EditorSelection, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
} from "@codemirror/view";
import type { EditResult } from "$lib/utils/vaultMarkdownEdit";
import type { FindMatch } from "$lib/utils/vaultFindInNote";

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

/** Caret coords relative to a shell element for slash menu anchoring. */
export function getCodeMirrorCaretAnchor(
  view: EditorView,
  relativeTo?: HTMLElement | null,
): { top: number; left: number } {
  const head = view.state.selection.main.head;
  const coords = view.coordsAtPos(head);
  if (!coords) return { top: 40, left: 16 };
  const origin = relativeTo?.getBoundingClientRect() ?? view.dom.getBoundingClientRect();
  return {
    top: Math.max(0, coords.bottom - origin.top + 4),
    left: Math.max(8, coords.left - origin.left),
  };
}

export const vaultEditorBaseTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "inherit",
  },
  ".cm-scroller": {
    overflow: "auto",
    fontFamily: "inherit",
    lineHeight: "inherit",
  },
  ".cm-content": {
    padding: "0",
    caretColor: "rgb(var(--color-primary-200))",
  },
  ".cm-gutters": {
    display: "none",
  },
  ".cm-activeLine": {
    backgroundColor: "transparent",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-vault-find-mark": {
    backgroundColor: "rgb(250 204 21 / 0.38)",
  },
  ".cm-vault-find-mark-active": {
    backgroundColor: "rgb(250 204 21 / 0.62)",
    boxShadow: "inset 0 -1.5px 0 0 rgb(234 179 8 / 0.85)",
  },
});
