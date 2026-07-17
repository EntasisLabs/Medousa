/**
 * Own ProseMirror scroll-into-view so typing doesn't jump the Live pane.
 * Returns true to suppress the default (often scrolls the wrong ancestor).
 */

import type { EditorView } from "@tiptap/pm/view";

const MARGIN_PX = 28;

export function handleLiveScrollToSelection(view: EditorView): boolean {
  const host = view.dom.closest(".vault-live-editor");
  if (!(host instanceof HTMLElement)) return true;

  const { from, to, empty } = view.state.selection;
  let top: number;
  let bottom: number;
  try {
    const start = view.coordsAtPos(from);
    const end = empty ? start : view.coordsAtPos(to);
    top = Math.min(start.top, end.top);
    bottom = Math.max(start.bottom, end.bottom);
  } catch {
    return true;
  }

  const hostRect = host.getBoundingClientRect();
  const visibleTop = hostRect.top + MARGIN_PX;
  const visibleBottom = hostRect.bottom - MARGIN_PX;

  if (top >= visibleTop && bottom <= visibleBottom) {
    return true;
  }

  if (top < visibleTop) {
    host.scrollTop += top - visibleTop;
  } else if (bottom > visibleBottom) {
    host.scrollTop += bottom - visibleBottom;
  }
  return true;
}
