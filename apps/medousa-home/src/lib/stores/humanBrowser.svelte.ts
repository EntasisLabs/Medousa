/** Local browser tabs. Native views own page history; the shell owns selection. */
import {
  humanBrowserActivateTab,
  humanBrowserCloseTab,
  humanBrowserEmbedHide,
  humanBrowserFindInPage,
  humanBrowserGoBack,
  humanBrowserGoForward,
  humanBrowserNavigate,
  humanBrowserQueryNavState,
  humanBrowserReload,
  humanBrowserStop,
  type HumanBrowserNavigatedPayload,
} from "$lib/humanBrowser";
import { browserHistory } from "$lib/browser/browserHistory.svelte";
import { browserPageLabel } from "$lib/utils/browserUrl";
import { resolveBrowserDestination } from "$lib/utils/resolveBrowserDestination";
import { dispatchBrowserFocusUrl } from "$lib/utils/browserChromeEvents";

export type HumanBrowserTab = {
  id: string;
  url: string;
  title: string;
  favicon?: string | null;
  active: boolean;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  error: string | null;
};

type SavedTab = Pick<HumanBrowserTab, "id" | "url" | "title" | "favicon">;
type SavedSession = { tabs: SavedTab[]; activeTabId: string; closedTabs?: SavedTab[] };
const EMBED_SESSION_KEY = "medousa-browser-session-embed";
const POPOUT_SESSION_KEY = "medousa-browser-session-popout";
const LEGACY_SESSION_KEY = "medousa-browser-session";
const MAX_CLOSED_TABS = 5;

function newTab(url = "about:blank", active = true): HumanBrowserTab {
  return {
    id: `tab-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    url, title: browserPageLabel(url), favicon: null, active,
    loading: false, canGoBack: false, canGoForward: false, error: null,
  };
}

function isValidUrl(url: unknown): url is string {
  if (url === "about:blank") return true;
  if (typeof url !== "string" || !url) return false;
  try { return ["http:", "https:"].includes(new URL(url).protocol); }
  catch { return false; }
}

function restoreTab(tab: SavedTab, active: boolean): HumanBrowserTab {
  return {
    ...newTab(tab.url, active), id: tab.id,
    title: typeof tab.title === "string" && tab.title.trim() ? tab.title : browserPageLabel(tab.url),
    favicon: typeof tab.favicon === "string" ? tab.favicon : null,
  };
}

function loadSession(key: string): { tabs: HumanBrowserTab[]; closedTabs: HumanBrowserTab[] } | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(key) ?? (key === EMBED_SESSION_KEY ? localStorage.getItem(LEGACY_SESSION_KEY) : null);
    if (!raw) return null;
    const saved = JSON.parse(raw) as SavedSession;
    if (!Array.isArray(saved.tabs)) return null;
    const ids = new Set<string>();
    const tabs = saved.tabs.filter((tab) => {
      if (!tab || typeof tab.id !== "string" || !tab.id || ids.has(tab.id) || !isValidUrl(tab.url)) return false;
      ids.add(tab.id);
      return true;
    }).map((tab) => restoreTab(tab, tab.id === saved.activeTabId));
    if (!tabs.length) return null;
    if (!tabs.some((tab) => tab.active)) tabs[tabs.length - 1]!.active = true;
    const closedTabs = Array.isArray(saved.closedTabs) ? saved.closedTabs
      .filter((tab) => tab && typeof tab.id === "string" && isValidUrl(tab.url))
      .slice(0, MAX_CLOSED_TABS).map((tab) => restoreTab(tab, false)) : [];
    return { tabs, closedTabs };
  } catch { return null; }
}

function saveTab({ id, url, title, favicon }: HumanBrowserTab): SavedTab {
  return { id, url, title, favicon };
}

export class HumanBrowserStore {
  private readonly sessionKey: string;
  private sessionDirty = false;
  private persistTimer: ReturnType<typeof setTimeout> | null = null;
  private nativeQueue: Promise<unknown> = Promise.resolve();
  private navigationVersions = new Map<string, number>();
  private navQueries = new Map<string, Promise<void>>();
  private pendingNavigation = new Set<string>();
  private closingTabs = new Set<string>();
  private lastReloadAt = new Map<string, number>();

  tabs = $state<HumanBrowserTab[]>([]);
  closedTabs = $state<HumanBrowserTab[]>([]);
  urlDraft = $state("");
  findOpen = $state(false);
  activeTab = $derived(this.tabs.find((tab) => tab.active) ?? this.tabs[0] ?? null);
  activeUrl = $derived(this.activeTab?.url ?? "about:blank");
  loading = $derived(this.activeTab?.loading ?? false);
  error = $derived(this.activeTab?.error ?? null);
  canGoBack = $derived(this.activeTab?.canGoBack ?? false);
  canGoForward = $derived(this.activeTab?.canGoForward ?? false);
  showStartPage = $derived(this.activeUrl === "about:blank");
  scopeLabel = $derived(this.activeTab?.title?.trim() || "Web");

  constructor(sessionKey: string) {
    this.sessionKey = sessionKey;
    const saved = loadSession(sessionKey);
    this.tabs = saved?.tabs ?? [newTab()];
    this.closedTabs = saved?.closedTabs ?? [];
    this.updateDraft();
    if (typeof window !== "undefined") window.addEventListener("pagehide", this.flushSession);
  }

  private updateDraft() { this.urlDraft = this.activeUrl === "about:blank" ? "" : this.activeUrl; }

  private updateTab(id: string, patch: Partial<HumanBrowserTab>) {
    const existing = this.tabs.find((tab) => tab.id === id);
    if (!existing || Object.entries(patch).every(([key, value]) => existing[key as keyof HumanBrowserTab] === value)) return;
    this.tabs = this.tabs.map((tab) => tab.id === id ? { ...tab, ...patch } : tab);
  }

  private persist() {
    this.sessionDirty = true;
    if (this.persistTimer) clearTimeout(this.persistTimer);
    this.persistTimer = setTimeout(this.flushSession, 200);
  }

  private flushSession = () => {
    if (this.persistTimer) clearTimeout(this.persistTimer);
    this.persistTimer = null;
    if (!this.sessionDirty || typeof localStorage === "undefined" || !this.activeTab) return;
    try {
      localStorage.setItem(this.sessionKey, JSON.stringify({
        tabs: this.tabs.map(saveTab), activeTabId: this.activeTab.id,
        closedTabs: this.closedTabs.map(saveTab),
      } satisfies SavedSession));
      this.sessionDirty = false;
    } catch { /* A full browser store must not interrupt live browsing. */ }
  };

  dispose() {
    this.flushSession();
    if (typeof window !== "undefined") window.removeEventListener("pagehide", this.flushSession);
  }

  private enqueueNative<T>(operation: () => Promise<T>): Promise<T> {
    const next = this.nativeQueue.then(operation);
    this.nativeQueue = next.catch(() => {});
    return next;
  }

  private version(id: string) { return this.navigationVersions.get(id) ?? 0; }
  private advance(id: string) {
    this.pendingNavigation.delete(id);
    const next = this.version(id) + 1;
    this.navigationVersions.set(id, next);
    return next;
  }

  private fail(id: string, error: unknown) {
    this.updateTab(id, { loading: false, error: error instanceof Error ? error.message : String(error) });
  }

  private mirrorActive(id: string) {
    if (this.sessionKey !== EMBED_SESSION_KEY) return;
    void import("$lib/stores/shellTabs.svelte").then(({ shellTabs }) => {
      if (this.activeTab?.id === id) shellTabs.mirrorWebTab(id, { activate: true });
    });
  }

  setLoading(loading: boolean, tabId?: string | null) {
    const id = tabId ?? this.activeTab?.id;
    if (id) this.updateTab(id, { loading });
  }

  setNativeNavState(canGoBack: boolean, canGoForward: boolean, tabId?: string | null) {
    const id = tabId ?? this.activeTab?.id;
    if (id) this.updateTab(id, { canGoBack, canGoForward });
  }

  async refreshNativeNavState(tabId = this.activeTab?.id): Promise<void> {
    if (!tabId || this.pendingNavigation.has(tabId)) return;
    const existing = this.navQueries.get(tabId);
    if (existing) return existing;
    const version = this.version(tabId);
    const query = (async () => {
      try {
        const state = await humanBrowserQueryNavState(tabId);
        if (version !== this.version(tabId) || !this.tabs.some((tab) => tab.id === tabId)) return;
        if (state.tabId && state.tabId !== tabId) return;
        if (!state.tabId && this.activeTab?.id !== tabId) return;
        this.setNativeNavState(state.canGoBack, state.canGoForward, tabId);
        if (typeof state.loading === "boolean") this.setLoading(state.loading, tabId);
        if (state.url && state.loading !== true && !this.tabs.find((tab) => tab.id === tabId)?.loading) {
          this.syncFromNative({ url: state.url, tabId, inPage: true }, false);
        }
      } catch { /* A closing/recreated native view is queried again when active. */ }
    })();
    this.navQueries.set(tabId, query);
    try { await query; } finally { if (this.navQueries.get(tabId) === query) this.navQueries.delete(tabId); }
  }

  /** Lightweight native state sync catches pushState/hash navigation without page scripts. */
  watchActiveNavigation(): () => void {
    void this.refreshNativeNavState();
    const timer = setInterval(() => { void this.refreshNativeNavState(); }, 500);
    return () => clearInterval(timer);
  }

  async syncActiveTabToNative() {
    const id = this.activeTab?.id;
    if (!id) return;
    try {
      await this.enqueueNative(async () => {
        const active = this.activeTab;
        if (!active || active.id !== id) return;
        await humanBrowserActivateTab(id, active.url);
        if (this.sessionKey !== POPOUT_SESSION_KEY && this.activeTab?.id === id && active.url === "about:blank") await humanBrowserEmbedHide();
      });
      await this.refreshNativeNavState(id);
    } catch (error) { this.fail(id, error); }
  }

  syncFromNative(payload: HumanBrowserNavigatedPayload, refresh = true) {
    const url = payload.url.trim();
    if (!isValidUrl(url)) return;
    const id = payload.tabId ?? this.activeTab?.id;
    const tab = this.tabs.find((entry) => entry.id === id);
    if (!tab) return;
    const changedUrl = url !== tab.url;
    const title = payload.title?.trim() || (changedUrl && !payload.inPage ? browserPageLabel(url) : tab.title);
    this.updateTab(tab.id, { url, title, favicon: payload.favicon ?? (changedUrl && !payload.inPage ? null : tab.favicon) });
    if (tab.active && changedUrl && (this.urlDraft === tab.url || !this.urlDraft)) this.updateDraft();
    if (changedUrl || title !== tab.title) {
      if (url !== "about:blank") browserHistory.record(url, title);
      this.persist();
    }
    if (refresh) void this.refreshNativeNavState(tab.id);
  }

  async navigate(input: string, _options?: { skipHistory?: boolean }) {
    const tab = this.activeTab;
    if (!tab || !input.trim()) return;
    const version = this.advance(tab.id);
    this.pendingNavigation.add(tab.id);
    this.updateTab(tab.id, { loading: true, error: null });
    try {
      const url = await resolveBrowserDestination(input.trim());
      if (this.version(tab.id) !== version || !this.tabs.some((entry) => entry.id === tab.id)) return;
      if (url === tab.url && url !== "about:blank") {
        await this.enqueueNative(async () => {
          if (this.version(tab.id) === version && this.tabs.some((entry) => entry.id === tab.id)) await humanBrowserReload(tab.id);
        });
      } else {
        this.updateTab(tab.id, { url, title: browserPageLabel(url) });
        if (this.activeTab?.id === tab.id) this.updateDraft();
        this.persist();
        await this.enqueueNative(async () => {
          if (this.version(tab.id) !== version || !this.tabs.some((entry) => entry.id === tab.id)) return;
          if (this.activeTab?.id === tab.id) await humanBrowserActivateTab(tab.id, url, true);
          else await humanBrowserNavigate(url, tab.id);
        });
      }
    } catch (error) { if (this.version(tab.id) === version) this.fail(tab.id, error); }
    finally { if (this.version(tab.id) === version) this.pendingNavigation.delete(tab.id); }
  }

  async openTab(url = "about:blank", saved?: SavedTab) {
    const tab = { ...newTab(url), title: saved?.title || browserPageLabel(url), favicon: saved?.favicon ?? null, loading: url !== "about:blank" };
    this.tabs = [...this.tabs.map((entry) => ({ ...entry, active: false })), tab];
    this.updateDraft();
    this.persist();
    this.mirrorActive(tab.id);
    await this.syncActiveTabToNative();
    if (url === "about:blank" && this.activeTab?.id === tab.id) dispatchBrowserFocusUrl();
  }

  async activateTab(tabId: string) {
    const target = this.tabs.find((tab) => tab.id === tabId);
    if (!target || target.active) return;
    this.tabs = this.tabs.map((tab) => ({ ...tab, active: tab.id === tabId }));
    this.updateDraft();
    this.persist();
    this.mirrorActive(tabId);
    await this.syncActiveTabToNative();
  }

  async closeTab(tabId: string) {
    const index = this.tabs.findIndex((tab) => tab.id === tabId);
    const closing = this.tabs[index];
    if (!closing || this.closingTabs.has(tabId)) return;
    this.closingTabs.add(tabId);
    this.advance(tabId);
    this.pendingNavigation.delete(tabId);
    this.closedTabs = [closing, ...this.closedTabs].slice(0, MAX_CLOSED_TABS);
    const remaining = this.tabs.filter((tab) => tab.id !== tabId);
    if (!remaining.length) remaining.push(newTab());
    if (closing.active) {
      const nextId = remaining[Math.min(index, remaining.length - 1)]!.id;
      this.tabs = remaining.map((tab) => ({ ...tab, active: tab.id === nextId }));
      this.updateDraft();
    } else this.tabs = remaining;
    this.persist();
    try {
      await this.enqueueNative(() => humanBrowserCloseTab(tabId));
      if (closing.active) await this.syncActiveTabToNative();
      this.navigationVersions.delete(tabId);
      this.lastReloadAt.delete(tabId);
    } catch (error) {
      this.tabs = [...this.tabs.slice(0, index), { ...closing, active: false, loading: false }, ...this.tabs.slice(index)];
      this.closedTabs = this.closedTabs.filter((tab) => tab.id !== tabId);
      this.fail(tabId, error);
      this.persist();
    } finally { this.closingTabs.delete(tabId); }
  }

  async reopenClosedTab() {
    const tab = this.closedTabs[0];
    if (!tab) return;
    this.closedTabs = this.closedTabs.slice(1);
    await this.openTab(tab.url, saveTab(tab));
  }

  async reload() {
    const tab = this.activeTab;
    if (!tab || tab.url === "about:blank") return;
    const now = Date.now();
    if (now - (this.lastReloadAt.get(tab.id) ?? -Infinity) < 450) return;
    this.lastReloadAt.set(tab.id, now);
    this.advance(tab.id);
    this.updateTab(tab.id, { loading: true, error: null });
    try {
      await this.enqueueNative(async () => {
        if (!this.tabs.some((entry) => entry.id === tab.id)) return;
        const current = this.tabs.find((entry) => entry.id === tab.id);
        if (!current) return;
        if (this.activeTab?.id === tab.id) await humanBrowserActivateTab(tab.id, current.url);
        await humanBrowserReload(tab.id);
      });
    } catch (error) { this.fail(tab.id, error); }
  }

  async stop() {
    const id = this.activeTab?.id;
    if (!id) return;
    this.advance(id);
    try { await this.enqueueNative(() => humanBrowserStop(id)); }
    catch (error) { this.fail(id, error); }
    finally { this.setLoading(false, id); }
  }

  private async traverse(direction: "back" | "forward") {
    const tab = this.activeTab;
    if (!tab || !(direction === "back" ? tab.canGoBack : tab.canGoForward)) return;
    this.advance(tab.id);
    this.updateTab(tab.id, { error: null });
    try {
      await this.enqueueNative(() => direction === "back" ? humanBrowserGoBack(tab.id) : humanBrowserGoForward(tab.id));
      await this.refreshNativeNavState(tab.id);
    } catch (error) { this.fail(tab.id, error); }
  }

  async goBack() { await this.traverse("back"); }
  async goForward() { await this.traverse("forward"); }
  async findInPage(query: string, forward = true) { return humanBrowserFindInPage(query, forward); }
  openFindBar() { this.findOpen = true; }
  closeFindBar() { this.findOpen = false; }
}

export const humanBrowserEmbed = new HumanBrowserStore(EMBED_SESSION_KEY);
export const humanBrowserPopout = new HumanBrowserStore(POPOUT_SESSION_KEY);
export const humanBrowser = humanBrowserEmbed;
