/** LME (Life Management Environment) — unified Vault + Automations workspace. */

import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
import { artifacts } from "$lib/stores/artifacts.svelte";
import { catalog } from "$lib/stores/catalog.svelte";
import { flows } from "$lib/stores/flows.svelte";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { externalDesk } from "$lib/stores/externalDesk.svelte";
import type { FlowComposerDraft } from "$lib/types/workflow";

export type LmeExplorerMode =
  | "notes"
  | "files"
  | "presentations"
  | "scripts"
  | "agents"
  | "flows"
  | "schedules"
  | "history";

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
    }
  | {
      tabId: string;
      kind: "flow";
      /** null = draft / new flow composer */
      workflowId: string | null;
      title: string;
    };

const EXPLORER_MODE_KEY = "medousa-lme-explorer-mode";
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

function noteTitle(path: string): string {
  return vault.labelByPathMap.get(path) ?? path.split("/").pop() ?? path;
}

function fileTitle(path: string): string {
  return path.split("/").pop()?.split("\\").pop() || path;
}

function newTabId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

function mirrorActiveTabToShell(tabId: string | null, title?: string) {
  if (!tabId) return;
  void import("$lib/stores/shellTabs.svelte").then(({ shellTabs }) => {
    shellTabs.mirrorLmeTab(tabId, { activate: true, title });
  });
}

export class LmeWorkspaceStore {
  explorerMode = $state<LmeExplorerMode>(loadExplorerMode());
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
        mirrorActiveTabToShell(existing.tabId, existing.title);
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
      if (activateMode) mirrorActiveTabToShell(tab.tabId, tab.title);
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
      mirrorActiveTabToShell(existing.tabId, existing.title);
    } else {
      const tab: LmeTab = {
        tabId: newTabId("file"),
        kind: "file",
        path,
        title,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
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
      mirrorActiveTabToShell(existing.tabId, label);
    } else {
      const tab: LmeTab = {
        tabId: newTabId("deck"),
        kind: "deck",
        artifactId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
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
      mirrorActiveTabToShell(existing.tabId, label);
    } else {
      const tab: LmeTab = {
        tabId: newTabId("manuscript"),
        kind: "manuscript",
        manuscriptId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
    }
    void catalog.loadManuscriptDetail(manuscriptId);
  }

  /** Focus the single draft flow tab (composer). Does not reset an already-seeded draft. */
  focusFlowComposerTab(title?: string) {
    this.setExplorerMode("flows");
    flows.composerOpen = true;
    const label = title?.trim() || flows.composerDraft.name.trim() || "New flow";
    const existing = this.tabs.find(
      (tab) => tab.kind === "flow" && tab.workflowId === null,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
      if (existing.title !== label) {
        this.tabs = this.tabs.map((tab) =>
          tab.tabId === existing.tabId ? { ...tab, title: label } : tab,
        );
      }
      mirrorActiveTabToShell(existing.tabId, label);
      return;
    }
    const tab: LmeTab = {
      tabId: newTabId("flow"),
      kind: "flow",
      workflowId: null,
      title: label,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    this.activeTabId = tab.tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
  }

  openNewFlow(seed?: Partial<FlowComposerDraft>) {
    flows.openComposer(seed);
    this.focusFlowComposerTab(seed?.name?.trim() || "New flow");
  }

  openFlow(workflowId: string, title: string) {
    this.setExplorerMode("flows");
    const label = title.trim() || workflowId;
    const existing = this.tabs.find(
      (tab) => tab.kind === "flow" && tab.workflowId === workflowId,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
      if (existing.title !== label) {
        this.tabs = this.tabs.map((tab) =>
          tab.tabId === existing.tabId ? { ...tab, title: label } : tab,
        );
      }
      mirrorActiveTabToShell(existing.tabId, label);
    } else {
      const tab: LmeTab = {
        tabId: newTabId("flow"),
        kind: "flow",
        workflowId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
    }
    void flows.loadDetail(workflowId);
    void flows.loadRuns(workflowId);
  }

  /** Keep draft tab title in sync with the composer name field. */
  syncFlowComposerTabTitle(title: string) {
    const label = title.trim() || "New flow";
    const existing = this.tabs.find(
      (tab) => tab.kind === "flow" && tab.workflowId === null,
    );
    if (!existing || existing.title === label) return;
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === existing.tabId ? { ...tab, title: label } : tab,
    );
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
        mirrorActiveTabToShell(existing.tabId, nextTitle);
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
    mirrorActiveTabToShell(tab.tabId, tab.title);
  }

  async activateTab(tabId: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabId);
    if (!tab) return;
    this.activeTabId = tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
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
    if (tab.kind === "flow") {
      this.setExplorerMode("flows");
      if (tab.workflowId) {
        void flows.loadDetail(tab.workflowId);
        void flows.loadRuns(tab.workflowId);
      } else {
        flows.composerOpen = true;
      }
      return;
    }
    this.setExplorerMode("presentations");
    artifacts.selectArtifact(tab.artifactId);
  }

  async closeTab(tabId: string, options?: { activateNext?: boolean }) {
    const closing = this.tabs.find((tab) => tab.tabId === tabId);
    if (!closing) return;
    const wasActive = this.activeTabId === tabId;
    this.tabs = this.tabs.filter((tab) => tab.tabId !== tabId);

    if (closing.kind === "script") {
      graphemeScriptEditor.closeTab(closing.scriptTabId);
    }
    if (closing.kind === "flow" && closing.workflowId === null) {
      flows.closeComposer();
    }
    if (closing.kind === "file" && vault.previewingAttachmentPath === closing.path) {
      vault.closeAttachmentPreview();
    }

    if (!wasActive) return;

    const next = this.tabs.at(-1) ?? null;
    this.activeTabId = next?.tabId ?? null;
    if (next && options?.activateNext !== false) {
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
