import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

/** Coordinates F2 / context-menu rename across LME + script tab strips. */
export class ScriptRenameUiStore {
  /** LME document tab id to rename. */
  lmeTabId = $state<string | null>(null);
  /** Grapheme editor tab id to rename. */
  editorTabId = $state<string | null>(null);
  /** Library list script id for inline rename in explorer. */
  libraryScriptId = $state<string | null>(null);
  /** Bumps so consumers can react even if id is unchanged. */
  token = $state(0);

  startActiveRename() {
    this.clear();
    const lme = lmeWorkspace.activeTab;
    if (lme?.kind === "script") {
      this.lmeTabId = lme.tabId;
      this.token += 1;
      return;
    }
    const tab = graphemeScriptEditor.activeTab;
    if (tab) {
      this.editorTabId = tab.tabId;
      this.token += 1;
    }
  }

  startLibraryRename(scriptId: string) {
    this.clear();
    this.libraryScriptId = scriptId;
    this.token += 1;
  }

  clear() {
    this.lmeTabId = null;
    this.editorTabId = null;
    this.libraryScriptId = null;
  }

  clearLme() {
    this.lmeTabId = null;
  }

  clearEditor() {
    this.editorTabId = null;
  }

  clearLibrary() {
    this.libraryScriptId = null;
  }
}

export const scriptRenameUi = new ScriptRenameUiStore();
