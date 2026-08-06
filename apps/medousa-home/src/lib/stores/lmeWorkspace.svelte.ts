/** LME (Life Management Environment) — unified Vault + Automations workspace. */

import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
import { artifacts } from "$lib/stores/artifacts.svelte";
import { catalog } from "$lib/stores/catalog.svelte";
import { automations } from "$lib/stores/automations.svelte";
import { flows } from "$lib/stores/flows.svelte";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { externalDesk } from "$lib/stores/externalDesk.svelte";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import type { FlowComposerDraft } from "$lib/types/workflow";
import { openCodeWorkspaceSession } from "$lib/utils/codeWorkspaceController";

export type LmeExplorerMode =
  | "notes"
  | "files"
  | "code"
  | "artifacts"
  | "scripts"
  | "agents"
  | "flows"
  | "schedules"
  | "history";

export type CodeWorkspaceResource =
  | { kind: "workspace" }
  | { kind: "file"; path: string; line: number | null }
  | { kind: "review" };

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
      scriptId: string | null;
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
      kind: "code";
      workId: string;
      title: string;
      resource: CodeWorkspaceResource;
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
    }
  | {
      tabId: string;
      kind: "schedule";
      recurringId: string;
      title: string;
    };

const EXPLORER_MODE_KEY = "medousa-lme-explorer-mode";
const MAX_TABS = 16;

export type LmeWorkspaceSession = {
  tabs: LmeTab[];
  activeTabId: string | null;
};

function isString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isRestorableTab(value: unknown): value is LmeTab {
  if (!value || typeof value !== "object") return false;
  const tab = value as Record<string, unknown>;
  if (!isString(tab.tabId) || !isString(tab.title) || !isString(tab.kind)) return false;
  switch (tab.kind) {
    case "note":
    case "file":
      return isString(tab.path);
    case "script":
      return isString(tab.scriptTabId) && isString(tab.scriptId);
    case "code": {
      if (!isString(tab.workId) || !tab.resource || typeof tab.resource !== "object") return false;
      const resource = tab.resource as Record<string, unknown>;
      return resource.kind === "workspace" || resource.kind === "review" || (
        resource.kind === "file" &&
        isString(resource.path) &&
        (resource.line === null || typeof resource.line === "number")
      );
    }
    case "deck":
      return isString(tab.artifactId);
    case "manuscript":
      return isString(tab.manuscriptId);
    case "flow":
      return tab.workflowId === null || isString(tab.workflowId);
    case "schedule":
      return isString(tab.recurringId);
    default:
      return false;
  }
}

function loadExplorerMode(): LmeExplorerMode {
  if (typeof localStorage === "undefined") return "notes";
  const raw = localStorage.getItem(EXPLORER_MODE_KEY);
  // Legacy Library tab id — Presentations was renamed to Artifacts.
  if (raw === "presentations") return "artifacts";
  if (
    raw === "notes" ||
    raw === "files" ||
    raw === "code" ||
    raw === "artifacts" ||
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

function codeResourceKey(workId: string, resource: CodeWorkspaceResource): string {
  if (resource.kind === "file") {
    return `code-file:${encodeURIComponent(workId)}:${encodeURIComponent(resource.path)}`;
  }
  return `code-${resource.kind}:${encodeURIComponent(workId)}`;
}

function mirrorActiveTabToShell(tabId: string | null, title?: string) {
  if (!tabId) return;
  void import("$lib/stores/shellTabs.svelte").then(({ shellTabs }) => {
    shellTabs.mirrorLmeTab(tabId, { activate: true, title });
  });
}

export class LmeWorkspaceStore {
  explorerMode = $state<LmeExplorerMode>(loadExplorerMode());
  codeCreateRequested = $state(false);
  tabs = $state<LmeTab[]>([]);
  activeTabId = $state<string | null>(null);

  activeTab = $derived(
    this.activeTabId
      ? (this.tabs.find((tab) => tab.tabId === this.activeTabId) ?? null)
      : null,
  );

  captureSession(): LmeWorkspaceSession {
    // Unsaved script drafts are editor-runtime state, not durable resources.
    const tabs = this.tabs
      .filter((tab) => tab.kind !== "script" || Boolean(tab.scriptId))
      .slice(-MAX_TABS);
    const ids = new Set(tabs.map((tab) => tab.tabId));
    return {
      tabs,
      activeTabId: this.activeTabId && ids.has(this.activeTabId)
        ? this.activeTabId
        : tabs.at(-1)?.tabId ?? null,
    };
  }

  restoreSession(value: unknown): LmeWorkspaceSession {
    const candidate = value && typeof value === "object"
      ? value as { tabs?: unknown; activeTabId?: unknown }
      : {};
    const seen = new Set<string>();
    const tabs = (Array.isArray(candidate.tabs) ? candidate.tabs : [])
      .filter(isRestorableTab)
      .filter((tab) => {
        if (seen.has(tab.tabId)) return false;
        seen.add(tab.tabId);
        return true;
      })
      .slice(-MAX_TABS);
    const activeTabId = typeof candidate.activeTabId === "string" && seen.has(candidate.activeTabId)
      ? candidate.activeTabId
      : tabs.at(-1)?.tabId ?? null;
    this.tabs = tabs;
    this.activeTabId = activeTabId;
    return { tabs, activeTabId };
  }

  updateCodeLocation(workId: string, path: string, line: number) {
    const nextLine = Math.max(1, Math.floor(line));
    let changed = false;
    const tabs = this.tabs.map((tab) => {
      if (
        tab.kind !== "code" ||
        tab.workId !== workId ||
        tab.resource.kind !== "file" ||
        tab.resource.path !== path ||
        tab.resource.line === nextLine
      ) {
        return tab;
      }
      changed = true;
      return { ...tab, resource: { ...tab.resource, line: nextLine } };
    });
    if (changed) this.tabs = tabs;
  }

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
    } else if (mode === "artifacts") {
      externalDesk.setSidebarMode("artifacts");
    }
  }

  requestNewCodeProject() {
    this.setExplorerMode("code");
    this.codeCreateRequested = true;
  }

  consumeNewCodeProjectRequest() {
    this.codeCreateRequested = false;
  }

  /** Map legacy Automations sections onto LME explorer modes. */
  openAutomationsSection(section: AutomationsSection) {
    this.setExplorerMode(section);
  }

  ensureNoteTabForSelection() {
    const path = vault.selectedPath;
    if (!path) return;
    this.ensureAndActivateNoteTab(path, { activate: false });
  }

  /**
   * Create or focus an LME note tab for `path` so the keep-alive host binds to
   * the vault lease. Used after createNote / openLooseFile (vault already open).
   */
  ensureAndActivateNoteTab(
    path: string,
    options?: { activate?: boolean; activateMode?: boolean },
  ) {
    const trimmed = path.trim();
    if (!trimmed) return;
    const activate = options?.activate !== false;
    const activateMode = options?.activateMode !== false;
    if (activate && activateMode) {
      this.setExplorerMode("notes");
    }
    const existing = this.tabs.find(
      (tab) => tab.kind === "note" && tab.path === trimmed,
    );
    if (existing) {
      if (activate) {
        this.activeTabId = existing.tabId;
        mirrorActiveTabToShell(existing.tabId, existing.title);
      } else if (this.explorerMode === "notes" && !this.activeTabId) {
        this.activeTabId = existing.tabId;
      }
      return;
    }
    const tab: LmeTab = {
      tabId: newTabId("note"),
      kind: "note",
      path: trimmed,
      title: noteTitle(trimmed),
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    if (activate || (this.explorerMode === "notes" && !this.activeTabId)) {
      this.activeTabId = tab.tabId;
      if (activate) mirrorActiveTabToShell(tab.tabId, tab.title);
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

  /** Open governed coding work as a durable workspace tab. */
  async openCodeWorkspace(workId: string, title?: string) {
    const id = workId.trim();
    if (!id) return;
    this.setExplorerMode("code");
    const result = await openCodeWorkspaceSession(id);
    if (result.ok) {
      return this.openCodeFile(id, result.path, { projectTitle: title });
    }

    // A project without a working copy still needs a real room so setup and
    // recovery actions have somewhere to render.
    const resource: CodeWorkspaceResource = { kind: "workspace" };
    const resourceId = codeResourceKey(id, resource);
    const existing = this.tabs.find(
      (tab) => tab.kind === "code" && tab.tabId === resourceId,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
      mirrorActiveTabToShell(existing.tabId, existing.title);
      return result;
    }
    const label = title?.trim() || "Code workspace";
    const tab: LmeTab = {
      tabId: resourceId,
      kind: "code",
      workId: id,
      title: label,
      resource,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    this.activeTabId = tab.tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
    return result;
  }

  /** Open a source buffer as a first-class Home workspace tab. */
  async openCodeFile(
    workId: string,
    path: string,
    options?: {
      line?: number | null;
      projectTitle?: string;
      activate?: boolean;
      recordNavigation?: boolean;
    },
  ) {
    const id = workId.trim();
    const normalizedPath = path.trim().replaceAll("\\", "/").replace(/^\.\//, "");
    if (!id || !normalizedPath) return null;
    this.setExplorerMode("code");
    const line =
      options?.line && options.line > 0 ? Math.floor(options.line) : null;
    const { codeWorkspace } = await import("$lib/stores/codeWorkspace.svelte");
    const source = await codeWorkspace.open(id, normalizedPath, line, {
      recordNavigation: options?.recordNavigation,
    });
    if (!source) return null;

    const resource: CodeWorkspaceResource = {
      kind: "file",
      path: normalizedPath,
      line,
    };
    const resourceId = codeResourceKey(id, resource);
    const label = source.title || fileTitle(normalizedPath);
    const existing = this.tabs.find(
      (tab) => tab.kind === "code" && tab.tabId === resourceId,
    );
    if (existing && existing.kind === "code") {
      if (
        existing.resource.kind === "file" &&
        existing.resource.line !== line
      ) {
        this.tabs = this.tabs.map((tab) =>
          tab.kind === "code" &&
          tab.tabId === resourceId &&
          tab.resource.kind === "file"
            ? { ...tab, resource: { ...tab.resource, line } }
            : tab,
        );
      }
      if (options?.activate !== false) {
        this.activeTabId = existing.tabId;
        mirrorActiveTabToShell(existing.tabId, existing.title);
      }
      return source;
    }

    const tab: LmeTab = {
      tabId: resourceId,
      kind: "code",
      workId: id,
      title: label,
      resource,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    if (options?.activate !== false) {
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
    }
    return source;
  }

  /** Review is a canvas, not a panel stacked beneath the editor. */
  async openCodeReview(workId: string, title?: string) {
    const id = workId.trim();
    if (!id) return;
    this.setExplorerMode("code");
    await openCodeWorkspaceSession(id);
    const resource: CodeWorkspaceResource = { kind: "review" };
    const resourceId = codeResourceKey(id, resource);
    const label = title?.trim() || "Review changes";
    const existing = this.tabs.find(
      (tab) => tab.kind === "code" && tab.tabId === resourceId,
    );
    if (existing) {
      this.activeTabId = existing.tabId;
      mirrorActiveTabToShell(existing.tabId, existing.title);
      return;
    }
    const tab: LmeTab = {
      tabId: resourceId,
      kind: "code",
      workId: id,
      title: label,
      resource,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    this.activeTabId = tab.tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
  }

  /** Close every shell presentation of a source resource, then release its buffer. */
  async closeCodeFile(workId: string, path: string) {
    const resourceId = codeResourceKey(workId, {
      kind: "file",
      path,
      line: null,
    });
    const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
    const shellIds = shellTabs.tabs
      .filter((tab) => tab.kind === "lme" && tab.lmeTabId === resourceId)
      .map((tab) => tab.id);
    if (shellIds.length > 0) {
      for (const shellId of shellIds) shellTabs.close(shellId);
      return;
    }
    await this.closeTab(resourceId);
  }

  async replaceCodeFile(
    workId: string,
    oldPath: string,
    newPath: string,
    line = 1,
  ) {
    await this.openCodeFile(workId, newPath, { line });
    await this.closeCodeFile(workId, oldPath);
  }

  openDeck(artifactId: string, title?: string) {
    this.setExplorerMode("artifacts");
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

  openSchedule(recurringId: string, title: string) {
    this.setExplorerMode("schedules");
    const label = title.trim() || recurringId;
    const existing = this.tabs.find(
      (tab) => tab.kind === "schedule" && tab.recurringId === recurringId,
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
        tabId: newTabId("schedule"),
        kind: "schedule",
        recurringId,
        title: label,
      };
      this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
      this.activeTabId = tab.tabId;
      mirrorActiveTabToShell(tab.tabId, tab.title);
    }
    void automations.loadRuns(recurringId);
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
      scriptId: scriptTab.scriptId,
      title: nextTitle,
    };
    this.tabs = [...this.tabs, tab].slice(-MAX_TABS);
    this.activeTabId = tab.tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
  }

  async activateTab(tabId: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabId);
    if (!tab) return;

    const leaving = this.activeTab;
    const leavingNote =
      leaving &&
      leaving.tabId !== tabId &&
      leaving.kind === "note" &&
      (tab.kind !== "note" || tab.path !== leaving.path);
    if (leavingNote) {
      // Flush TipTap/CM + save BEFORE swapping activeTabId (remount hammer).
      const ok = await vault.flushBeforeLeave();
      if (!ok) return;
    }

    this.activeTabId = tabId;
    mirrorActiveTabToShell(tab.tabId, tab.title);
    // Do not setExplorerMode here — rail mode is user-driven so browsing
    // Local Files / Notes survives tab and shell switches.
    if (tab.kind === "note") {
      // Absolute paths route to openLooseFile inside openNote (skipLeaveFlush preserved).
      await vault.openNote(tab.path, {
        skipLeaveFlush: Boolean(leavingNote),
      });
      return;
    }
    if (tab.kind === "script") {
      if (graphemeScriptEditor.tabs.some((entry) => entry.tabId === tab.scriptTabId)) {
        graphemeScriptEditor.selectTab(tab.scriptTabId);
      } else if (tab.scriptId) {
        await graphemeScriptEditor.openScriptById(tab.scriptId);
        const restored = graphemeScriptEditor.activeTab;
        if (restored) {
          this.tabs = this.tabs.map((entry) =>
            entry.tabId === tab.tabId && entry.kind === "script"
              ? { ...entry, scriptTabId: restored.tabId, scriptId: restored.scriptId }
              : entry,
          );
        }
      }
      return;
    }
    if (tab.kind === "file") {
      externalDesk.selectExternalPath(tab.path);
      vault.previewAttachment(tab.path, "pane");
      return;
    }
    if (tab.kind === "code") {
      const { undertakings } = await import("$lib/stores/undertakings.svelte");
      if (undertakings.detail?.id !== tab.workId) {
        await undertakings.select(tab.workId);
      }
      if (tab.resource.kind === "file") {
        const { codeWorkspace } = await import("$lib/stores/codeWorkspace.svelte");
        await codeWorkspace.hydrate(tab.workId);
        await codeWorkspace.open(
          tab.workId,
          tab.resource.path,
          tab.resource.line,
        );
        undertakings.setSelection({
          path: tab.resource.path,
          line: tab.resource.line,
          entityId: null,
        });
      }
      return;
    }
    if (tab.kind === "manuscript") {
      void catalog.loadManuscriptDetail(tab.manuscriptId);
      return;
    }
    if (tab.kind === "flow") {
      if (tab.workflowId) {
        void flows.loadDetail(tab.workflowId);
        void flows.loadRuns(tab.workflowId);
      } else {
        flows.composerOpen = true;
      }
      return;
    }
    if (tab.kind === "schedule") {
      void automations.loadRuns(tab.recurringId);
      return;
    }
    artifacts.selectArtifact(tab.artifactId);
  }

  confirmCloseTab(tabId: string): boolean {
    const closing = this.tabs.find((tab) => tab.tabId === tabId);
    if (
      !closing ||
      closing.kind !== "code" ||
      closing.resource.kind !== "file"
    ) {
      return true;
    }
    const closingPath = closing.resource.path;
    const buffer = codeWorkspace.tabsFor(closing.workId).find(
      (tab) => tab.path === closingPath,
    );
    return !(
      buffer &&
      codeWorkspace.isDirty(buffer) &&
      typeof window !== "undefined" &&
      !window.confirm(`Discard unsaved changes to ${buffer.path}?`)
    );
  }

  async closeTab(
    tabId: string,
    options?: { activateNext?: boolean; confirmed?: boolean },
  ) {
    const closing = this.tabs.find((tab) => tab.tabId === tabId);
    if (!closing) return;
    if (!options?.confirmed && !this.confirmCloseTab(tabId)) return;
    if (closing.kind === "code" && closing.resource.kind === "file") {
      const { codeWorkspace } = await import("$lib/stores/codeWorkspace.svelte");
      const closingPath = closing.resource.path;
      const buffer = codeWorkspace.tabsFor(closing.workId).find(
        (tab) => tab.path === closingPath,
      );
      if (buffer) codeWorkspace.close(buffer.tabId);
    }
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
