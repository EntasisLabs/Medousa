/** LME (Life Management Environment) — unified Vault + Automations workspace. */

import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
import { artifacts } from "$lib/stores/artifacts.svelte";
import { catalog } from "$lib/stores/catalog.svelte";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { externalDesk } from "$lib/stores/externalDesk.svelte";

export type LmeExplorerMode =
  | "notes"
  | "files"
  | "presentations"
  | "scripts"
  | "agents"
  | "flows"
  | "schedules"
  | "history";

export type LmeScriptsExplorerSection = "scripts" | "templates" | "modules" | "wasm";

export type LmeTab =
  | {
      tabId: string;
      kind: "note";
      path: string;
      title: string;
    }
  | {
      tabId: string;
      kind: "script";
      scriptTabId: string;
      title: string;
    }
  | {
      tabId: string;
      kind: "file";
      path: string;
      title: string;
    }
  | {
      tabId: string;
      kind: "deck";
      artifactId: string;
      title: string;
    }
  | {
      tabId: string;
      kind: "manuscript";
      manuscriptId: string;
      title: string;
    };

const EXPLORER_MODE_KEY = "medousa-lme-explorer-mode";
const SCRIPTS_SECTION_KEY = "medousa-lme-scripts-section";
const MAX_TABS = 16;

function loadExplorerMode(): LmeExplorerMode {
  if (typeof localStorage === "undefined") return "notes";
  const raw = localStorage.getItem(EXPLORER_MODE_KEY);
  if (
    raw === "notes" ||
    raw === "files" ||
    raw === "presentations" ||
    raw === "scripts" ||
    raw === "agents" ||
    raw === "flows" ||
    raw === "schedules" ||
    raw === "history"
  ) {
    return raw;
  }
  return "notes";
}

function loadScriptsSection(): LmeScriptsExplorerSection {
  if (typeof localStorage === "undefined") return "scripts";
  const raw = localStorage.getItem(SCRIPTS_SECTION_KEY);
  if (
    raw === "scripts" ||
    raw === "templates" ||
    raw === "modules" ||
    raw === "wasm"
  ) {
    return raw;
  }
  return "scripts";
}

function noteTitle(path: string): string {
  return vault.labelByPathMap.get(path) ?? path.split("/").pop() ?? path;
}

function fileTitle(path: string): string {
  return path.split("/").pop()?.split("\\").pop() || path;
}

function newTabId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

export class LmeWorkspaceStore {
  explorerMode = $state<LmeExplorerMode>(loadExplorerMode());
  scriptsExplorerSection = $state<LmeScriptsExplorerSection>(loadScriptsSection());
  tabs = $state<LmeTab[]>([]);
  activeTabId = $state<string | null>(null);

  activeTab = $derived(
    this.activeTabId
      ? (this.tabs.find((tab) => tab.tabId === this.activeTabId) ?? null)
      : null,
  );

  /** Mode bar only — never steals the active document tab. */
  setExplorerMode(mode: LmeExplorerMode) {
    this.explorerMode = mode;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(EXPLORER_MODE_KEY, mode);
    }
    if (mode === "notes") {
      externalDesk.setSidebarMode("vault");
    } else if (mode === "files") {
      externalDesk.setSidebarMode("files");
    } else if (mode === "presentations") {
      externalDesk.setSidebarMode("presentations");
    }
  }

  setScriptsExplorerSection(section: LmeScriptsExplorerSection) {
    this.scriptsExplorerSection = section;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(SCRIPTS_SECTION_KEY, section);
    }
  }

  /** Map legacy Automations sections onto LME explorer modes. */
  openAutomationsSection(section: AutomationsSection) {
    this.setExplorerMode(section);
  }

  ensureNoteTabForSelection() {
    const path = vault.selectedPath;
    if (!path) return;
    const existing = this.tabs.find(
      (tab) => tab.kind === "note" && tab.path === path,
    );
    if (existing) {
      if (this.explorerMode === "notes" && !this.activeTabId) {
        this.activeTabId = existing.tabId;
      }
      return;
    }
    const tab: LmeTab = {
      tabId: newTabId("note"),
      kind: "note",
      path,
      title: noteTitle(path),
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    if (this.explorerMode === "notes" && !this.activeTabId) {
      this.activeTabId = tab.tabId;
    }
  }

  /**
   * Open a note into the workspace tab strip.
   * Pass `{ activateMode: false }` for hydration so a slow refresh cannot yank
   * the explorer mode after the user left Notes.
   */
  async openNote(path: string, options?: { activateMode?: boolean }) {
    const activateMode = options?.activateMode !== false;
    if (activateMode) {
      this.setExplorerMode("notes");
    }
    const existing = this.tabs.find(
      (tab) => tab.kind === "note" && tab.path === path,
    );
    if (existing) {
      if (activateMode) {
        this.activeTabId = existing.tabId;
      }
      await vault.openNote(path);
      return;
    }
    await vault.openNote(path);
    const tab: LmeTab = {
      tabId: newTabId("note"),
      kind: "note",
      path,
      title: noteTitle(path),
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    if (activateMode || !this.activeTabId) {
      this.activeTabId = tab.tabId;
    }
  }

  async openScriptById(scriptId: string) {
    this.setExplorerMode("scripts");
    await graphemeScriptEditor.openScriptById(scriptId);
    this.syncScriptTabFromEditor({ activate: true });
  }

  openNewScript() {
    this.setExplorerMode("scripts");
    graphemeScriptEditor.openNewTab();
    this.syncScriptTabFromEditor({ activate: true });
  }

  openFile(path: string, options?: { activateMode?: boolean }) {
    const activateMode = options?.activateMode !== false;
    if (activateMode) {
      this.setExplorerMode("files");
    }
    const title = fileTitle(path);
    const existing = this.tabs.find(
      (tab) => tab.kind === "file" && tab.path === path,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
    } else {
      const tab: LmeTab = {
        tabId: newTabId("file"),
        kind: "file",
        path,
        title,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
    }
    externalDesk.selectExternalPath(path);
    vault.previewAttachment(path, "pane");
  }

  openDeck(artifactId: string, title?: string) {
    this.setExplorerMode("presentations");
    const existing = this.tabs.find(
      (tab) => tab.kind === "deck" && tab.artifactId === artifactId,
    );
    const label =
      title?.trim() ||
      artifacts.artifacts.find((row) => row.artifact_id === artifactId)?.label ||
      "Presentation";
    if (existing) {
      this.activeTabId = existing.tabId;
      if (existing.title !== label) {
        this.tabs = this.tabs.map((tab) =>
          tab.tabId === existing.tabId ? { ...tab, title: label } : tab,
        );
      }
    } else {
      const tab: LmeTab = {
        tabId: newTabId("deck"),
        kind: "deck",
        artifactId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
    }
    artifacts.selectArtifact(artifactId);
  }

  openManuscript(manuscriptId: string, title: string) {
    this.setExplorerMode("agents");
    const label = title.trim() || manuscriptId;
    const existing = this.tabs.find(
      (tab) => tab.kind === "manuscript" && tab.manuscriptId === manuscriptId,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
      if (existing.title !== label) {
        this.tabs = this.tabs.map((tab) =>
          tab.tabId === existing.tabId ? { ...tab, title: label } : tab,
        );
      }
    } else {
      const tab: LmeTab = {
        tabId: newTabId("manuscript"),
        kind: "manuscript",
        manuscriptId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
    }
    void catalog.loadManuscriptDetail(manuscriptId);
  }

  /** Mirror the active grapheme editor tab into the LME strip. Idempotent. */
  syncScriptTabFromEditor(options?: { activate?: boolean }) {
    const scriptTab = graphemeScriptEditor.activeTab;
    if (!scriptTab) return;
    const nextTitle = scriptTab.name || "Untitled script";
    const activate = options?.activate === true;
    const existing = this.tabs.find(
      (tab) => tab.kind === "script" && tab.scriptTabId === scriptTab.tabId,
    );
    if (existing) {
      const titleChanged = existing.title !== nextTitle;
      const activeChanged = activate && this.activeTabId !== existing.tabId;
      if (!titleChanged && !activeChanged) return;
      if (titleChanged) {
        this.tabs = this.tabs.map((tab) =>
          tab.tabId === existing.tabId ? { ...tab, title: nextTitle } : tab,
        );
      }
      if (activeChanged) {
        this.activeTabId = existing.tabId;
      }
      return;
    }
    // Background sync (titles) must not resurrect tabs after the strip was emptied.
    if (!activate) return;
    const tab: LmeTab = {
      tabId: newTabId("script"),
      kind: "script",
      scriptTabId: scriptTab.tabId,
      title: nextTitle,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    this.activeTabId = tab.tabId;
  }

  async activateTab(tabId: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabId);
    if (!tab) return;
    this.activeTabId = tabId;
    if (tab.kind === "note") {
      this.setExplorerMode("notes");
      await vault.openNote(tab.path);
      return;
    }
    if (tab.kind === "script") {
      this.setExplorerMode("scripts");
      graphemeScriptEditor.selectTab(tab.scriptTabId);
      return;
    }
    if (tab.kind === "file") {
      this.setExplorerMode("files");
      externalDesk.selectExternalPath(tab.path);
      vault.previewAttachment(tab.path, "pane");
      return;
    }
    if (tab.kind === "manuscript") {
      this.setExplorerMode("agents");
      void catalog.loadManuscriptDetail(tab.manuscriptId);
      return;
    }
    this.setExplorerMode("presentations");
    artifacts.selectArtifact(tab.artifactId);
  }

  async closeTab(tabId: string) {
    const closing = this.tabs.find((tab) => tab.tabId === tabId);
    if (!closing) return;
    const wasActive = this.activeTabId === tabId;
    this.tabs = this.tabs.filter((tab) => tab.tabId !== tabId);

    if (closing.kind === "script") {
      graphemeScriptEditor.closeTab(closing.scriptTabId);
    }
    if (closing.kind === "file" && vault.previewingAttachmentPath === closing.path) {
      vault.closeAttachmentPreview();
    }

    if (!wasActive) return;

    const next = this.tabs.at(-1) ?? null;
    this.activeTabId = next?.tabId ?? null;
    if (next) {
      await this.activateTab(next.tabId);
    }
  }

  /** Refresh note tab titles when vault labels change. Idempotent. */
  refreshNoteTitles() {
    let changed = false;
    const next = this.tabs.map((tab) => {
      if (tab.kind !== "note") return tab;
      const title = noteTitle(tab.path);
      if (title === tab.title) return tab;
      changed = true;
      return { ...tab, title };
    });
    if (changed) this.tabs = next;
  }
}

export const lmeWorkspace = new LmeWorkspaceStore();
