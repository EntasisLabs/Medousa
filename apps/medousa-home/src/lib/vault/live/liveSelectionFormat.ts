/** Selection format helpers for Live TipTap. */

import type { Editor } from "@tiptap/core";
import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
import type { MarkdownColorToken } from "$lib/utils/vaultMarkdownColors";

export type SelectionAnchor = {
  left: number;
  top: number;
  width: number;
  height: number;
};

/** Nonempty text selection suitable for the format bubble (not atom-only). */
export function liveSelectionHasText(editor: Editor): boolean {
  const { empty, from, to } = editor.state.selection;
  if (empty || from === to) return false;
  const text = editor.state.doc.textBetween(from, to, " ");
  return text.trim().length > 0;
}

export function liveSelectionAnchor(editor: Editor): SelectionAnchor | null {
  if (!liveSelectionHasText(editor)) return null;
  const view = editor.view;
  const { from, to } = editor.state.selection;
  try {
    const start = view.coordsAtPos(from);
    const end = view.coordsAtPos(to);
    const left = Math.min(start.left, end.left);
    const right = Math.max(start.right, end.right);
    const top = Math.min(start.top, end.top);
    const bottom = Math.max(start.bottom, end.bottom);
    return {
      left,
      top,
      width: Math.max(1, right - left),
      height: Math.max(1, bottom - top),
    };
  } catch {
    return null;
  }
}

export function liveActiveFormatActions(editor: Editor): MarkdownFormatAction[] {
  const active: MarkdownFormatAction[] = [];
  if (editor.isActive("bold")) active.push("bold");
  if (editor.isActive("italic")) active.push("italic");
  if (editor.isActive("code")) active.push("code");
  if (editor.isActive("highlight")) active.push("highlight");
  if (editor.isActive("heading", { level: 1 })) active.push("h1");
  if (editor.isActive("heading", { level: 2 })) active.push("h2");
  if (editor.isActive("heading", { level: 3 })) active.push("h3");
  if (editor.isActive("bulletList")) active.push("bullet");
  if (editor.isActive("orderedList")) active.push("numbered");
  if (editor.isActive("taskList")) active.push("checkbox");
  if (editor.isActive("link")) active.push("link");
  return active;
}

/** Restore a stored range before formatting (bubble clicks can clear selection). */
export function restoreLiveSelection(
  editor: Editor,
  from: number,
  to: number,
): boolean {
  const size = editor.state.doc.content.size;
  const nextFrom = Math.max(0, Math.min(from, size));
  const nextTo = Math.max(0, Math.min(to, size));
  if (nextFrom === nextTo) return false;
  return editor.chain().focus().setTextSelection({ from: nextFrom, to: nextTo }).run();
}

export function applyLiveFormatAction(
  editor: Editor,
  action: MarkdownFormatAction,
  range?: { from: number; to: number } | null,
): boolean {
  if (range && range.from !== range.to) {
    restoreLiveSelection(editor, range.from, range.to);
  } else if (!liveSelectionHasText(editor)) {
    return false;
  }

  const chain = editor.chain().focus();
  switch (action) {
    case "bold":
      return chain.toggleBold().run();
    case "italic":
      return chain.toggleItalic().run();
    case "code":
      return chain.toggleCode().run();
    case "highlight":
      return chain.toggleHighlight().run();
    case "h1":
      return chain.toggleHeading({ level: 1 }).run();
    case "h2":
      return chain.toggleHeading({ level: 2 }).run();
    case "h3":
      return chain.toggleHeading({ level: 3 }).run();
    case "bullet":
      return chain.toggleBulletList().run();
    case "numbered":
      return chain.toggleOrderedList().run();
    case "checkbox":
      return chain.toggleTaskList().run();
    case "link": {
      const prev = editor.getAttributes("link").href as string | undefined;
      const href = window.prompt("Link URL", prev ?? "https://");
      if (href === null) return true;
      if (!href) return chain.extendMarkRange("link").unsetLink().run();
      return chain.extendMarkRange("link").setLink({ href }).run();
    }
    default:
      return false;
  }
}

export function applyLiveTextColor(
  editor: Editor,
  color: MarkdownColorToken,
  range?: { from: number; to: number } | null,
): boolean {
  if (range && range.from !== range.to) {
    restoreLiveSelection(editor, range.from, range.to);
  } else if (!liveSelectionHasText(editor)) {
    return false;
  }

  const current = editor.getAttributes("textColor").color as string | undefined;
  if (current && String(current).toLowerCase() === String(color).toLowerCase()) {
    return editor.chain().focus().unsetTextColor().run();
  }
  return editor.chain().focus().setTextColor(color).run();
}
