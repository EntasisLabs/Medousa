import {
  getCodeEditorLanguage,
  resolveCodeEditorLanguage,
  type CodeEditorLanguageId,
} from "$lib/code/codeEditorLanguageRegistry";
import { getGraphemeScript } from "$lib/daemon";
import { randomUuid } from "$lib/utils/randomUuid";

export interface GraphemeLspWorkspaceResponse {
  root_path: string;
  root_uri: string;
  scripts_dir: string;
}

export interface ScriptEditorTab {
  tabId: string;
  scriptId: string | null;
  name: string;
  body: string;
  intent: string;
  tags: string[];
  dirty: boolean;
  version: number;
  /** Editor language plug-in id (default grapheme script). */
  languageId: CodeEditorLanguageId;
}

function newTabId(): string {
  return `tab-${randomUuid()}`;
}

function untitledName(existing: ScriptEditorTab[]): string {
  const used = new Set(existing.map((tab) => tab.name.toLowerCase()));
  let index = 1;
  while (used.has(`untitled-${index}`)) {
    index += 1;
  }
  return `Untitled ${index}`;
}

export class GraphemeScriptEditorStore {
  tabs = $state<ScriptEditorTab[]>([]);
  activeTabId = $state<string | null>(null);
  lspWorkspace = $state<GraphemeLspWorkspaceResponse | null>(null);
  lspReady = $state(false);
  sidePane = $state<"diagnostics" | "info">("diagnostics");
  pendingInsert = $state<string | null>(null);
  compileResult = $state<import("$lib/types/grapheme").GraphemeCompileResponse | null>(
    null,
  );
  compileBusy = $state(false);
  compileError = $state<string | null>(null);
  saveBusy = $state(false);
  saveError = $state<string | null>(null);
  runBusy = $state(false);
  runError = $state<string | null>(null);
  statusMessage = $state<string | null>(null);
  /** Bumped when body is replaced from outside the editor (templates, library open). */
  contentEpoch = $state(0);

  /**
   * Prefer getters over class-field `$derived` — the latter can freeze after the
   * effect that owned construction is destroyed (mobile More → Automations remounts).
   */
  get activeTab(): ScriptEditorTab | null {
    const id = this.activeTabId;
    if (!id) return null;
    return this.tabs.find((tab) => tab.tabId === id) ?? null;
  }

  documentUriForTab(tab: ScriptEditorTab): string | null {
    if (!getCodeEditorLanguage(tab.languageId).capabilities.lsp) return null;
    if (!this.lspWorkspace) return null;
    const ext = getCodeEditorLanguage(tab.languageId).fileExtension ?? "grapheme";
    const fileName = tab.scriptId
      ? `${tab.scriptId}.${ext}`
      : `${tab.tabId}.${ext}`;
    const path = `${this.lspWorkspace.scripts_dir}/${fileName}`.replace(/\\/g, "/");
    if (path.startsWith("/")) {
      return `file://${path}`;
    }
    return `file:///${path}`;
  }

  get activeDocumentUri(): string | null {
    const tab = this.activeTab;
    return tab ? this.documentUriForTab(tab) : null;
  }

  ensureInitialTab() {
    if (this.tabs.length === 0) {
      this.openNewTab();
      return;
    }
    const id = this.activeTabId;
    if (!id || !this.tabs.some((tab) => tab.tabId === id)) {
      this.activeTabId = this.tabs[0].tabId;
    }
  }

  openNewTab(languageId: CodeEditorLanguageId = "grapheme") {
    const tab: ScriptEditorTab = {
      tabId: newTabId(),
      scriptId: null,
      name: untitledName(this.tabs),
      body: "",
      intent: "",
      tags: [],
      dirty: false,
      version: 0,
      languageId: resolveCodeEditorLanguage(languageId),
    };
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.tabId;
    this.compileResult = null;
    this.compileError = null;
    this.saveError = null;
    this.runError = null;
    this.contentEpoch += 1;
  }

  openScript(entry: {
    id: string;
    name: string;
    body: string;
    intent?: string | null;
    tags?: string[];
    version: number;
  }) {
    const existing = this.tabs.find((tab) => tab.scriptId === entry.id);
    if (existing) {
      this.tabs = this.tabs.map((tab) =>
        tab.tabId === existing.tabId
          ? {
              ...tab,
              name: entry.name,
              body: entry.body,
              intent: entry.intent ?? "",
              tags: entry.tags ?? [],
              dirty: false,
              version: entry.version,
              languageId: "grapheme" as const,
            }
          : tab,
      );
      this.activeTabId = existing.tabId;
      this.compileResult = null;
      this.compileError = null;
      this.contentEpoch += 1;
      return;
    }

    const loneEmptyTab =
      this.tabs.length === 1 &&
      !this.tabs[0].scriptId &&
      !this.tabs[0].dirty &&
      !this.tabs[0].body.trim();

    if (loneEmptyTab) {
      const tab = this.tabs[0];
      this.tabs = [
        {
          ...tab,
          scriptId: entry.id,
          name: entry.name,
          body: entry.body,
          intent: entry.intent ?? "",
          tags: entry.tags ?? [],
          dirty: false,
          version: entry.version,
          languageId: "grapheme",
        },
      ];
      this.activeTabId = tab.tabId;
      this.compileResult = null;
      this.compileError = null;
      this.contentEpoch += 1;
      return;
    }

    const tab: ScriptEditorTab = {
      tabId: newTabId(),
      scriptId: entry.id,
      name: entry.name,
      body: entry.body,
      intent: entry.intent ?? "",
      tags: entry.tags ?? [],
      dirty: false,
      version: entry.version,
      languageId: "grapheme",
    };
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.tabId;
    this.compileResult = null;
    this.compileError = null;
    this.contentEpoch += 1;
  }

  closeTab(tabId: string) {
    const next = this.tabs.filter((tab) => tab.tabId !== tabId);
    this.tabs = next;
    if (this.activeTabId === tabId) {
      this.activeTabId = next.at(-1)?.tabId ?? null;
    }
  }

  selectTab(tabId: string) {
    this.activeTabId = tabId;
    this.compileResult = null;
    this.compileError = null;
  }

  patchActiveTab(patch: Partial<Pick<ScriptEditorTab, "name" | "body" | "intent" | "tags">>) {
    this.ensureInitialTab();
    const id = this.activeTabId;
    if (!id) return;
    this.patchTab(id, patch);
  }

  /** Replace active tab content from a template / external source (forces editor remount). */
  loadExternalContent(patch: Partial<Pick<ScriptEditorTab, "name" | "body" | "intent" | "tags">>) {
    this.ensureInitialTab();
    let id = this.activeTabId;
    if (!id) {
      this.openNewTab();
      id = this.activeTabId;
    }
    if (!id) return;
    this.patchTab(id, patch);
    this.contentEpoch += 1;
  }

  patchTab(
    tabId: string,
    patch: Partial<Pick<ScriptEditorTab, "name" | "body" | "intent" | "tags">>,
  ) {
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === tabId
        ? {
            ...tab,
            ...patch,
            dirty:
              patch.body !== undefined && patch.body !== tab.body
                ? true
                : tab.dirty ||
                  (patch.name !== undefined && patch.name !== tab.name) ||
                  (patch.intent !== undefined && patch.intent !== tab.intent) ||
                  (patch.tags !== undefined &&
                    patch.tags.join(",") !== tab.tags.join(",")),
          }
        : tab,
    );
  }

  markActiveSaved(entry: {
    id: string;
    name: string;
    version: number;
  }) {
    const active = this.activeTab;
    if (!active) return;
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === active.tabId
        ? {
            ...tab,
            scriptId: entry.id,
            name: entry.name,
            dirty: false,
            version: entry.version,
          }
        : tab,
    );
    this.statusMessage = `Saved ${entry.name}`;
  }

  /**
   * Open a highlight-only or stub snippet tab (no vault/git, no fake LSP).
   * Grapheme scripts should use openScript / openNewTab instead.
   */
  openLanguageSnippet(input: {
    languageId: CodeEditorLanguageId | string;
    name?: string;
    body?: string;
  }) {
    const languageId = resolveCodeEditorLanguage(input.languageId);
    const def = getCodeEditorLanguage(languageId);
    const tab: ScriptEditorTab = {
      tabId: newTabId(),
      scriptId: null,
      name: input.name?.trim() || `${def.label} snippet`,
      body: input.body ?? "",
      intent: "",
      tags: [],
      dirty: Boolean(input.body),
      version: 0,
      languageId,
    };
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.tabId;
    this.compileResult = null;
    this.compileError = null;
    this.saveError = null;
    this.runError = null;
    if (def.tier === "highlight") {
      this.statusMessage = `${def.label} — highlight only`;
    } else {
      this.statusMessage = null;
    }
  }

  async openScriptById(scriptId: string) {
    const detail = await getGraphemeScript(scriptId);
    this.openScript({
      id: detail.script.id,
      name: detail.script.name,
      body: detail.body_preview,
      intent: detail.script.intent,
      tags: detail.script.tags,
      version: detail.script.version,
    });
  }

  queueInsert(text: string) {
    this.pendingInsert = text;
  }

  clearPendingInsert() {
    this.pendingInsert = null;
  }

  appendToActiveBody(text: string) {
    const active = this.activeTab;
    if (!active || !text) return;
    const separator =
      active.body.length === 0 ? "" : active.body.endsWith("\n") ? "" : "\n";
    this.patchActiveTab({ body: `${active.body}${separator}${text}` });
  }
}

export const graphemeScriptEditor = new GraphemeScriptEditorStore();
