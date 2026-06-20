import { getGraphemeScript } from "$lib/daemon";

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
}

function newTabId(): string {
  return `tab-${crypto.randomUUID()}`;
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
  sidePane = $state<"diagnostics" | "info" | "modules">("diagnostics");
  pendingInsert = $state<string | null>(null);
  modulesPaneModuleId = $state<string | null>(null);
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

  activeTab = $derived(
    this.activeTabId
      ? (this.tabs.find((tab) => tab.tabId === this.activeTabId) ?? null)
      : null,
  );

  documentUriForTab(tab: ScriptEditorTab): string | null {
    if (!this.lspWorkspace) return null;
    const fileName = tab.scriptId
      ? `${tab.scriptId}.grapheme`
      : `${tab.tabId}.grapheme`;
    const path = `${this.lspWorkspace.scripts_dir}/${fileName}`.replace(/\\/g, "/");
    if (path.startsWith("/")) {
      return `file://${path}`;
    }
    return `file:///${path}`;
  }

  activeDocumentUri = $derived(
    this.activeTab ? this.documentUriForTab(this.activeTab) : null,
  );

  ensureInitialTab() {
    if (this.tabs.length === 0) {
      this.openNewTab();
    }
  }

  openNewTab() {
    const tab: ScriptEditorTab = {
      tabId: newTabId(),
      scriptId: null,
      name: untitledName(this.tabs),
      body: "",
      intent: "",
      tags: [],
      dirty: false,
      version: 0,
    };
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.tabId;
    this.compileResult = null;
    this.compileError = null;
    this.saveError = null;
    this.runError = null;
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
      this.activeTabId = existing.tabId;
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
        },
      ];
      this.activeTabId = tab.tabId;
      this.compileResult = null;
      this.compileError = null;
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
    };
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.tabId;
    this.compileResult = null;
    this.compileError = null;
  }

  closeTab(tabId: string) {
    const next = this.tabs.filter((tab) => tab.tabId !== tabId);
    this.tabs = next;
    if (this.activeTabId === tabId) {
      this.activeTabId = next.at(-1)?.tabId ?? null;
      if (next.length === 0) {
        this.openNewTab();
      }
    }
  }

  selectTab(tabId: string) {
    this.activeTabId = tabId;
    this.compileResult = null;
    this.compileError = null;
  }

  patchActiveTab(patch: Partial<Pick<ScriptEditorTab, "name" | "body" | "intent" | "tags">>) {
    const active = this.activeTab;
    if (!active) return;
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === active.tabId
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
