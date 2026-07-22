/**
 * Suppress ProseMirror's default scroll-into-view (it often scrolls the wrong
 * ancestor). Do not nudge scrollTop here — any edge-following on the type path
 * becomes a typewriter fight with layout/reflow.
 *
 * Intentional jumps (find, wikilink heading scroll) use their own helpers.
 */

import type { EditorView } from "@tiptap/pm/view";

export function handleLiveScrollToSelection(_view: EditorView): boolean {
  return true;
}
