import {
  getCodeWorkspaceState,
  getUndertakingSource,
  saveCodeWorkspaceState,
  type ForgeSourceFile,
} from "$lib/forge";
import { resolveCodeEditorLanguage } from "$lib/code/codeEditorLanguageRegistry";

export type CodeDocumentTab = ForgeSourceFile & {
  tabId: string;
  title: string;
  language: string;
  draft: string;
  loading: boolean;
  error: string | null;
  syncKey: number;
  line: number | null;
};

function tabId(workId: string, path: string): string {
  return `source:${encodeURIComponent(workId)}:${encodeURIComponent(path)}`;
}

function titleFor(path: string): string {
  return path.split("/").pop() || path;
}

function languageFor(path: string): string {
  return resolveCodeEditorLanguage(path.split(".").pop()?.toLowerCase() ?? "");
}

class CodeWorkspaceStore {
  tabs = $state<CodeDocumentTab[]>([]);
  activeByWorkId = $state<Record<string, string | null>>({});
  secondaryByWorkId = $state<Record<string, string | null>>({});
  workspaceErrorByWorkId = $state<Record<string, string | null>>({});
  leaseByWorkId = $state<
    Record<string, { lease_id: string; generation: number } | null>
  >({});
  private hydrated = new Set<string>();
  private hydrating = new Map<string, Promise<void>>();
  private persistTimers = new Map<string, ReturnType<typeof setTimeout>>();

  tabsFor(workId: string): CodeDocumentTab[] {
    return this.tabs.filter((tab) => tab.work_id === workId);
  }

  activeFor(workId: string): CodeDocumentTab | null {
    const id = this.activeByWorkId[workId];
    return id ? (this.tabs.find((tab) => tab.tabId === id) ?? null) : null;
  }

  secondaryFor(workId: string): CodeDocumentTab | null {
    const id = this.secondaryByWorkId[workId];
    return id ? (this.tabs.find((tab) => tab.tabId === id) ?? null) : null;
  }

  isDirty(tab: CodeDocumentTab): boolean {
    return tab.draft !== tab.content;
  }

  patch(tabIdValue: string, patch: Partial<CodeDocumentTab>) {
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === tabIdValue ? { ...tab, ...patch } : tab,
    );
  }

  async open(
    workId: string,
    path: string,
    line?: number | null,
    options?: { persist?: boolean },
  ) {
    const id = tabId(workId, path);
    const existing = this.tabs.find((tab) => tab.tabId === id);
    const current = this.activeByWorkId[workId] ?? null;
    if (this.secondaryByWorkId[workId] === id && current) {
      this.secondaryByWorkId = { ...this.secondaryByWorkId, [workId]: current };
    }
    this.activeByWorkId = { ...this.activeByWorkId, [workId]: id };
    if (existing) {
      if (line && line > 0) this.patch(id, { line: Math.floor(line) });
      if (options?.persist !== false) this.schedulePersist(workId);
      return existing;
    }
    const placeholder: CodeDocumentTab = {
      tabId: id,
      work_id: workId,
      path,
      title: titleFor(path),
      language: languageFor(path),
      content: "",
      draft: "",
      digest: "",
      byte_size: 0,
      loading: true,
      error: null,
      syncKey: 0,
      line: line && line > 0 ? Math.floor(line) : null,
    };
    this.tabs = [...this.tabs, placeholder];
    const loaded = await this.reload(id, { discardDirty: true });
    if (options?.persist !== false) this.schedulePersist(workId);
    return loaded;
  }

  async reload(tabIdValue: string, options?: { discardDirty?: boolean }) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return null;
    if (this.isDirty(tab) && !options?.discardDirty) return tab;
    this.patch(tabIdValue, { loading: true, error: null });
    try {
      const source = await getUndertakingSource(tab.work_id, tab.path);
      const current = this.tabs.find((entry) => entry.tabId === tabIdValue);
      if (!current) return null;
      const next: CodeDocumentTab = {
        ...current,
        ...source,
        title: titleFor(source.path),
        language: languageFor(source.path),
        draft: source.content,
        loading: false,
        error: null,
        syncKey: current.syncKey + 1,
      };
      this.tabs = this.tabs.map((entry) =>
        entry.tabId === tabIdValue ? next : entry,
      );
      return next;
    } catch (err) {
      this.patch(tabIdValue, {
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
      return null;
    }
  }

  activate(tabIdValue: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    const current = this.activeByWorkId[tab.work_id] ?? null;
    if (this.secondaryByWorkId[tab.work_id] === tabIdValue && current) {
      this.secondaryByWorkId = {
        ...this.secondaryByWorkId,
        [tab.work_id]: current,
      };
    }
    this.activeByWorkId = {
      ...this.activeByWorkId,
      [tab.work_id]: tabIdValue,
    };
    this.schedulePersist(tab.work_id);
  }

  openToSide(tabIdValue: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab || this.activeByWorkId[tab.work_id] === tabIdValue) return;
    this.secondaryByWorkId = {
      ...this.secondaryByWorkId,
      [tab.work_id]: tabIdValue,
    };
    this.schedulePersist(tab.work_id);
  }

  closeSide(workId: string) {
    this.secondaryByWorkId = { ...this.secondaryByWorkId, [workId]: null };
    this.schedulePersist(workId);
  }

  updateDraft(tabIdValue: string, draft: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    this.patch(tabIdValue, { draft });
    this.schedulePersist(tab.work_id);
  }

  updateLine(tabIdValue: string, line: number) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    const next = Math.max(1, Math.floor(line));
    if (!tab || tab.line === next) return;
    this.patch(tabIdValue, { line: next });
    this.schedulePersist(tab.work_id);
  }

  acceptSaved(tabIdValue: string, source: ForgeSourceFile) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    this.patch(tabIdValue, {
      ...source,
      content: source.content,
      draft: source.content,
      error: null,
      syncKey: tab.syncKey + 1,
    });
    this.schedulePersist(tab.work_id);
  }

  setError(tabIdValue: string, error: string | null) {
    this.patch(tabIdValue, { error });
  }

  replacePath(workId: string, oldPath: string, source: ForgeSourceFile) {
    const oldId = tabId(workId, oldPath);
    const nextId = tabId(workId, source.path);
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === oldId
        ? {
            ...tab,
            ...source,
            tabId: nextId,
            title: titleFor(source.path),
            language: languageFor(source.path),
            content: source.content,
            draft: source.content,
            error: null,
            syncKey: tab.syncKey + 1,
          }
        : tab,
    );
    if (this.activeByWorkId[workId] === oldId) {
      this.activeByWorkId = { ...this.activeByWorkId, [workId]: nextId };
    }
    if (this.secondaryByWorkId[workId] === oldId) {
      this.secondaryByWorkId = { ...this.secondaryByWorkId, [workId]: nextId };
    }
    this.schedulePersist(workId);
  }

  removePath(workId: string, path: string) {
    this.close(tabId(workId, path));
  }

  close(tabIdValue: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    const workTabs = this.tabsFor(tab.work_id);
    const index = workTabs.findIndex((entry) => entry.tabId === tabIdValue);
    this.tabs = this.tabs.filter((entry) => entry.tabId !== tabIdValue);
    if (this.secondaryByWorkId[tab.work_id] === tabIdValue) {
      this.secondaryByWorkId = {
        ...this.secondaryByWorkId,
        [tab.work_id]: null,
      };
    }
    if (this.activeByWorkId[tab.work_id] !== tabIdValue) {
      this.schedulePersist(tab.work_id);
      return;
    }
    const next = workTabs[index + 1] ?? workTabs[index - 1] ?? null;
    this.activeByWorkId = {
      ...this.activeByWorkId,
      [tab.work_id]: next?.tabId ?? null,
    };
    if (next?.tabId === this.secondaryByWorkId[tab.work_id]) {
      this.secondaryByWorkId = {
        ...this.secondaryByWorkId,
        [tab.work_id]: null,
      };
    }
    this.schedulePersist(tab.work_id);
  }

  setLease(
    workId: string,
    lease: { lease_id: string; generation: number } | null,
  ) {
    this.leaseByWorkId = { ...this.leaseByWorkId, [workId]: lease };
  }

  hydrate(workId: string): Promise<void> {
    if (this.hydrated.has(workId)) return Promise.resolve();
    const existing = this.hydrating.get(workId);
    if (existing) return existing;
    const pending = this.loadPersisted(workId)
      .then(() => {
        this.workspaceErrorByWorkId = {
          ...this.workspaceErrorByWorkId,
          [workId]: null,
        };
      })
      .catch((err) => {
        this.workspaceErrorByWorkId = {
          ...this.workspaceErrorByWorkId,
          [workId]: `Could not restore Code workspace: ${err instanceof Error ? err.message : String(err)}`,
        };
      })
      .finally(() => {
        this.hydrating.delete(workId);
        this.hydrated.add(workId);
      });
    this.hydrating.set(workId, pending);
    return pending;
  }

  private async loadPersisted(workId: string) {
    const state = await getCodeWorkspaceState(workId);
    const restored = await Promise.all(
      state.tabs.slice(0, 32).map(async (saved): Promise<CodeDocumentTab | null> => {
        try {
          const source = await getUndertakingSource(workId, saved.path);
          const hasDraft = typeof saved.draft === "string";
          return {
            ...source,
            tabId: tabId(workId, source.path),
            title: titleFor(source.path),
            language: languageFor(source.path),
            draft: hasDraft ? saved.draft! : source.content,
            loading: false,
            error:
              hasDraft && saved.source_digest !== source.digest
                ? "Recovered draft is based on an older file version. Review before saving."
                : null,
            syncKey: 1,
            line: saved.line && saved.line > 0 ? Math.floor(saved.line) : null,
          };
        } catch {
          return null;
        }
      }),
    );
    const currentIds = new Set(this.tabs.map((tab) => tab.tabId));
    this.tabs = [
      ...this.tabs,
      ...restored.filter(
        (tab): tab is CodeDocumentTab => Boolean(tab && !currentIds.has(tab.tabId)),
      ),
    ];
    const active = restored.find((tab) => tab?.path === state.active_path) ?? restored[0];
    if (active) {
      this.activeByWorkId = {
        ...this.activeByWorkId,
        [workId]: active.tabId,
      };
    }
    const secondary = restored.find((tab) => tab?.path === state.secondary_path);
    if (secondary && secondary.tabId !== active?.tabId) {
      this.secondaryByWorkId = {
        ...this.secondaryByWorkId,
        [workId]: secondary.tabId,
      };
    }
  }

  private schedulePersist(workId: string) {
    const previous = this.persistTimers.get(workId);
    if (previous) clearTimeout(previous);
    this.persistTimers.set(
      workId,
      setTimeout(() => {
        this.persistTimers.delete(workId);
        void this.persist(workId);
      }, 700),
    );
  }

  async persist(workId: string) {
    if (!this.hydrated.has(workId)) return;
    const tabs = this.tabsFor(workId).filter((tab) => !tab.loading && tab.digest);
    const dirty = tabs.some((tab) => this.isDirty(tab));
    const lease = this.leaseByWorkId[workId] ?? null;
    if (dirty && !lease) return;
    try {
      await saveCodeWorkspaceState(
        workId,
        {
          tabs: tabs.map((tab) => ({
            path: tab.path,
            draft: this.isDirty(tab) ? tab.draft : null,
            source_digest: tab.digest,
            line: tab.line,
          })),
          active_path: this.activeFor(workId)?.path ?? null,
          secondary_path: this.secondaryFor(workId)?.path ?? null,
        },
        lease,
      );
    } catch (err) {
      const active = this.activeFor(workId);
      if (active) {
        this.setError(active.tabId, err instanceof Error ? err.message : String(err));
      }
    }
  }
}

export const codeWorkspace = new CodeWorkspaceStore();
