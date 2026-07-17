import type { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";

/**
 * VS Code / Cursor-style caret navigation for Live (TipTap).
 *
 * Line: Home / End, Cmd/Ctrl+←/→ (Mac line), Shift variants select.
 * Doc:  Mod+Home / Mod+End, Cmd+↑/↓ (Mac), Shift variants select.
 * Home is “smart”: first non-whitespace, then absolute line start.
 */

function moveCaret(editor: Editor, pos: number, select: boolean): boolean {
  const { from, anchor } = editor.state.selection;
  const next = Math.max(1, Math.min(pos, editor.state.doc.content.size));
  if (!select && from === next && editor.state.selection.empty) return true;
  if (select) {
    return editor
      .chain()
      .focus(undefined, { scrollIntoView: false })
      .setTextSelection({ from: anchor, to: next })
      .run();
  }
  return editor
    .chain()
    .focus(undefined, { scrollIntoView: false })
    .setTextSelection(next)
    .run();
}

function lineHomePos(editor: Editor): number | null {
  const { $from } = editor.state.selection;
  if (!$from.parent.isTextblock) return null;
  const start = $from.start();
  const indent = ($from.parent.textContent.match(/^\s*/)?.[0].length ?? 0);
  const soft = start + indent;
  // Smart Home: toggle between first non-ws and absolute start (VS Code).
  return $from.pos === soft ? start : soft;
}

function lineEndPos(editor: Editor): number | null {
  const { $from } = editor.state.selection;
  if (!$from.parent.isTextblock) return null;
  return $from.end();
}

function docStartPos(editor: Editor): number {
  return TextSelection.atStart(editor.state.doc).from;
}

function docEndPos(editor: Editor): number {
  return TextSelection.atEnd(editor.state.doc).from;
}

function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent);
}

/** Handle nav keys. Returns true when consumed. */
export function handleLiveNavKey(editor: Editor, event: KeyboardEvent): boolean {
  const key = event.key;
  const shift = event.shiftKey;
  const mod = event.metaKey || event.ctrlKey;
  const alt = event.altKey;
  const mac = isMacPlatform();

  // Physical Home / End (+ Mod = document)
  if (key === "Home" && !alt) {
    event.preventDefault();
    if (mod) return moveCaret(editor, docStartPos(editor), shift);
    const pos = lineHomePos(editor);
    return pos == null ? false : moveCaret(editor, pos, shift);
  }
  if (key === "End" && !alt) {
    event.preventDefault();
    if (mod) return moveCaret(editor, docEndPos(editor), shift);
    const pos = lineEndPos(editor);
    return pos == null ? false : moveCaret(editor, pos, shift);
  }

  // Mac / VS Code: Cmd+←/→ line, Cmd+↑/↓ document
  if (mac && event.metaKey && !event.ctrlKey && !alt) {
    if (key === "ArrowLeft") {
      event.preventDefault();
      const pos = lineHomePos(editor);
      return pos == null ? false : moveCaret(editor, pos, shift);
    }
    if (key === "ArrowRight") {
      event.preventDefault();
      const pos = lineEndPos(editor);
      return pos == null ? false : moveCaret(editor, pos, shift);
    }
    if (key === "ArrowUp") {
      event.preventDefault();
      return moveCaret(editor, docStartPos(editor), shift);
    }
    if (key === "ArrowDown") {
      event.preventDefault();
      return moveCaret(editor, docEndPos(editor), shift);
    }
  }

  // Win/Linux VS Code: Ctrl+Home/End already covered via Home+mod above.
  // Ctrl+↑/↓ are often page-scroll in editors — leave alone.

  return false;
}
