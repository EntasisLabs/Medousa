/**
 * Lightweight find/replace panel state for the Scripts CodeMirror host.
 * Distinct from vaultFind (notes) — same UX shape, separate store.
 */

import { openSearchPanel, closeSearchPanel, searchPanelOpen } from "@codemirror/search";
import type { EditorView } from "@codemirror/view";

class CodeEditorFindStore {
  open = $state(false);

  toggle(view: EditorView | null | undefined) {
    if (!view) return;
    if (searchPanelOpen(view.state)) {
      closeSearchPanel(view);
      this.open = false;
    } else {
      openSearchPanel(view);
      this.open = true;
    }
  }

  show(view: EditorView | null | undefined) {
    if (!view) return;
    openSearchPanel(view);
    this.open = true;
  }

  hide(view: EditorView | null | undefined) {
    if (!view) return;
    closeSearchPanel(view);
    this.open = false;
  }

  syncFromView(view: EditorView | null | undefined) {
    this.open = view ? searchPanelOpen(view.state) : false;
  }
}

export const codeEditorFind = new CodeEditorFindStore();
