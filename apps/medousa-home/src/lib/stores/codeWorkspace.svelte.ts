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

export type CodeLocation = { path: string; line: number | null };

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
  workspaceErrorByWorkId = $state<Record<string, string | null>>({});
  leaseByWorkId = $state<
    Record<string, { lease_id: string; generation: number } | null>
  >({});
  navigationByWorkId = $state<Record<string, CodeLocation[]>>({});
  navigationIndexByWorkId = $state<Record<string, number>>({});
  recentTabIdsByWorkId = $state<Record<string, string[]>>({});
  /** Most-recently-closed tabs per work item (path + line) for reopen. */
  closedByWorkId = $state<Record<string, CodeLocation[]>>({});
  /** Session tab order overrides keyed by work id. */
  tabOrderByWorkId = $state<Record<string, string[]>>({});
  private hydrated = new Set<string>();
  private hydrating = new Map<string, Promise<void>>();
  private opening = new Map<string, Promise<CodeDocumentTab | null>>();
  private persistTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private workspaceEpoch = 0;

  resetForWorkshopSwitch() {
    this.workspaceEpoch += 1;
    for (const timer of this.persistTimers.values()) clearTimeout(timer);
    this.persistTimers.clear();
    this.hydrated.clear();
    this.hydrating.clear();
    this.opening.clear();
    this.tabs = [];
    this.activeByWorkId = {};
    this.workspaceErrorByWorkId = {};
    this.leaseByWorkId = {};
    this.navigationByWorkId = {};
    this.navigationIndexByWorkId = {};
    this.recentTabIdsByWorkId = {};
    this.closedByWorkId = {};
    this.tabOrderByWorkId = {};
  }

  tabsFor(workId: string): CodeDocumentTab[] {
    const tabs = this.tabs.filter((tab) => tab.work_id === workId);
    const order = this.tabOrderByWorkId[workId];
    if (!order?.length) return tabs;
    const byId = new Map(tabs.map((tab) => [tab.tabId, tab]));
    const ordered: CodeDocumentTab[] = [];
    for (const id of order) {
      const tab = byId.get(id);
      if (tab) {
        ordered.push(tab);
        byId.delete(id);
      }
    }
    for (const tab of byId.values()) ordered.push(tab);
    return ordered;
  }

  orderedTabsFor(workId: string): CodeDocumentTab[] {
    return this.tabsFor(workId);
  }


  canReopenClosed(workId: string): boolean {
    return (this.closedByWorkId[workId]?.length ?? 0) > 0;
  }

  async reopenClosed(workId: string) {
    const stack = this.closedByWorkId[workId] ?? [];
    const location = stack[0];
    if (!location) return null;
    this.closedByWorkId = {
      ...this.closedByWorkId,
      [workId]: stack.slice(1),
    };
    return this.open(workId, location.path, location.line);
  }

  private rememberClosed(tab: CodeDocumentTab) {
    const entry: CodeLocation = {
      path: tab.path,
      line: tab.line && tab.line > 0 ? tab.line : null,
    };
    const previous = this.closedByWorkId[tab.work_id] ?? [];
    this.closedByWorkId = {
      ...this.closedByWorkId,
      [tab.work_id]: [
        entry,
        ...previous.filter((item) => item.path !== entry.path),
      ].slice(0, 32),
    };
  }

  activeFor(workId: string): CodeDocumentTab | null {
    const id = this.activeByWorkId[workId];
    return id ? (this.tabs.find((tab) => tab.tabId === id) ?? null) : null;
  }


  isDirty(tab: CodeDocumentTab): boolean {
    return tab.draft !== tab.content;
  }

  patch(tabIdValue: string, patch: Partial<CodeDocumentTab>) {
    const current = this.tabs.find((tab) => tab.tabId === tabIdValue);
    if (!current || Object.entries(patch).every(([key, value]) => current[key as keyof CodeDocumentTab] === value)) {
      return;
    }
    this.tabs = this.tabs.map((tab) =>
      tab.tabId === tabIdValue ? { ...tab, ...patch } : tab,
    );
  }

  async open(
    workId: string,
    path: string,
    line?: number | null,
    options?: { persist?: boolean; recordNavigation?: boolean },
  ) {
    const id = tabId(workId, path);
    const existing = this.tabs.find((tab) => tab.tabId === id);
    if (this.activeByWorkId[workId] !== id) {
      this.activeByWorkId = { ...this.activeByWorkId, [workId]: id };
      this.markRecent(workId, id);
    }
    if (options?.recordNavigation !== false) this.recordNavigation(workId, path, line ?? null);
    if (existing) {
      if (line && line > 0) this.patch(id, { line: Math.floor(line) });
      if (options?.persist !== false) this.schedulePersist(workId);
      return this.opening.get(id) ?? existing;
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
    const pending = (async () => {
      const loaded = await this.reload(id, { discardDirty: true });
      if (options?.persist !== false) this.schedulePersist(workId);
      return loaded;
    })().finally(() => {
      if (this.opening.get(id) === pending) this.opening.delete(id);
    });
    this.opening.set(id, pending);
    return pending;
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
    this.activeByWorkId = {
      ...this.activeByWorkId,
      [tab.work_id]: tabIdValue,
    };
    this.markRecent(tab.work_id, tabIdValue);
    this.recordNavigation(tab.work_id, tab.path, tab.line);
    this.schedulePersist(tab.work_id);
  }

  private markRecent(workId: string, tabIdValue: string) {
    const recent = this.recentTabIdsByWorkId[workId] ?? [];
    this.recentTabIdsByWorkId = {
      ...this.recentTabIdsByWorkId,
      [workId]: [tabIdValue, ...recent.filter((id) => id !== tabIdValue)].slice(0, 32),
    };
  }

  recentTabsFor(workId: string): CodeDocumentTab[] {
    const byId = new Map(this.tabsFor(workId).map((tab) => [tab.tabId, tab]));
    const ordered = (this.recentTabIdsByWorkId[workId] ?? [])
      .map((id) => byId.get(id))
      .filter((tab): tab is CodeDocumentTab => Boolean(tab));
    for (const tab of byId.values()) {
      if (!ordered.some((entry) => entry.tabId === tab.tabId)) ordered.push(tab);
    }
    return ordered;
  }

  private recordNavigation(workId: string, path: string, line: number | null) {
    const history = this.navigationByWorkId[workId] ?? [];
    const index = this.navigationIndexByWorkId[workId] ?? history.length - 1;
    const current = history[index];
    const next = { path, line: line && line > 0 ? Math.floor(line) : null };
    if (current?.path === next.path && current.line === next.line) return;
    const entries = [...history.slice(0, index + 1), next].slice(-100);
    this.navigationByWorkId = { ...this.navigationByWorkId, [workId]: entries };
    this.navigationIndexByWorkId = {
      ...this.navigationIndexByWorkId,
      [workId]: entries.length - 1,
    };
  }

  canNavigate(workId: string, direction: -1 | 1): boolean {
    const entries = this.navigationByWorkId[workId] ?? [];
    const index = this.navigationIndexByWorkId[workId] ?? entries.length - 1;
    return direction < 0 ? index > 0 : index >= 0 && index < entries.length - 1;
  }

  async navigate(workId: string, direction: -1 | 1) {
    if (!this.canNavigate(workId, direction)) return null;
    const entries = this.navigationByWorkId[workId] ?? [];
    const current = this.navigationIndexByWorkId[workId] ?? entries.length - 1;
    const index = current + direction;
    const location = entries[index];
    this.navigationIndexByWorkId = { ...this.navigationIndexByWorkId, [workId]: index };
    return this.open(workId, location.path, location.line, { recordNavigation: false });
  }


  updateDraft(tabIdValue: string, draft: string) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    this.patch(tabIdValue, { draft });
    // Draft durability is an idle checkpoint, never part of the keystroke path.
    this.schedulePersist(tab.work_id, 3_000);
  }

  updateLine(tabIdValue: string, line: number) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    const next = Math.max(1, Math.floor(line));
    if (!tab || tab.line === next) return;
    this.patch(tabIdValue, { line: next });
    this.schedulePersist(tab.work_id, 1_000);
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

  rebaseDraft(tabIdValue: string, source: ForgeSourceFile) {
    const tab = this.tabs.find((entry) => entry.tabId === tabIdValue);
    if (!tab) return;
    this.patch(tabIdValue, {
      ...source,
      draft: tab.draft,
      error: null,
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
    this.rememberClosed(tab);
    this.tabs = this.tabs.filter((entry) => entry.tabId !== tabIdValue);
    this.recentTabIdsByWorkId = {
      ...this.recentTabIdsByWorkId,
      [tab.work_id]: (this.recentTabIdsByWorkId[tab.work_id] ?? []).filter(
        (id) => id !== tabIdValue,
      ),
    };
    const order = this.tabOrderByWorkId[tab.work_id];
    if (order) {
      this.tabOrderByWorkId = {
        ...this.tabOrderByWorkId,
        [tab.work_id]: order.filter((id) => id !== tabIdValue),
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
    this.schedulePersist(tab.work_id);
  }

  setLease(
    workId: string,
    lease: { lease_id: string; generation: number } | null,
  ) {
    const current = this.leaseByWorkId[workId] ?? null;
    if (current?.lease_id === lease?.lease_id && current?.generation === lease?.generation) return;
    this.leaseByWorkId = { ...this.leaseByWorkId, [workId]: lease };
  }

  hydrate(workId: string): Promise<void> {
    if (this.hydrated.has(workId)) return Promise.resolve();
    const existing = this.hydrating.get(workId);
    if (existing) return existing;
    const epoch = this.workspaceEpoch;
    const pending = this.loadPersisted(workId, epoch)
      .then(() => {
        if (epoch !== this.workspaceEpoch) return;
        this.workspaceErrorByWorkId = {
          ...this.workspaceErrorByWorkId,
          [workId]: null,
        };
      })
      .catch((err) => {
        if (epoch !== this.workspaceEpoch) return;
        this.workspaceErrorByWorkId = {
          ...this.workspaceErrorByWorkId,
          [workId]: `Could not restore Code workspace: ${err instanceof Error ? err.message : String(err)}`,
        };
      })
      .finally(() => {
        if (epoch !== this.workspaceEpoch) return;
        this.hydrating.delete(workId);
        this.hydrated.add(workId);
      });
    this.hydrating.set(workId, pending);
    return pending;
  }

  private async loadPersisted(workId: string, epoch: number) {
    let state: Awaited<ReturnType<typeof getCodeWorkspaceState>>;
    try {
      state = await getCodeWorkspaceState(workId);
    } catch (err) {
      const status = (err as { status?: number } | null)?.status;
      const message = err instanceof Error ? err.message : String(err);
      // Older daemons lack workspace-state; treat as empty rather than blocking Code.
      if (
        status === 404 ||
        status === 405 ||
        /HTTP\s+404\b/i.test(message) ||
        /HTTP\s+405\b/i.test(message)
      ) {
        return;
      }
      throw err;
    }
    const restored = await Promise.all(
      state.tabs.slice(0, 12).map(async (saved): Promise<CodeDocumentTab | null> => {
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
    if (epoch !== this.workspaceEpoch) return;
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
  }

  private schedulePersist(workId: string, delay = 700) {
    const previous = this.persistTimers.get(workId);
    if (previous) clearTimeout(previous);
    this.persistTimers.set(
      workId,
      setTimeout(() => {
        this.persistTimers.delete(workId);
        void this.persist(workId);
      }, delay),
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
          secondary_path: null,
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
