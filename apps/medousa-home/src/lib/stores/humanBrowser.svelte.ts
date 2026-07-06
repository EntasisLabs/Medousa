/** Local-only browser state — separate sessions for main embed vs pop-out window. */

import {
  humanBrowserActivateTab,
  humanBrowserCloseTab,
  humanBrowserFindInPage,
  humanBrowserGoBack,
  humanBrowserGoForward,
  humanBrowserNavigate,
  humanBrowserQueryNavState,
  humanBrowserReload,
  humanBrowserStop,
  type HumanBrowserNavigatedPayload,
} from "$lib/humanBrowser";
import { browserHistory } from "$lib/stores/browserHistory.svelte";
import { browserPageLabel } from "$lib/utils/browserUrl";
import { resolveBrowserDestination } from "$lib/utils/resolveBrowserDestination";

export type HumanBrowserTab = {
  id: string;
  url: string;
  title: string;
  favicon?: string | null;
  active: boolean;
  historyBack: string[];
  historyForward: string[];
};

const EMBED_SESSION_KEY = "medousa-browser-session-embed";
const POPOUT_SESSION_KEY = "medousa-browser-session-popout";
const LEGACY_SESSION_KEY = "medousa-browser-session";
const MAX_TABS = 12;
const MAX_CLOSED_TABS = 5;

function tabLabelFromUrl(url: string, title?: string | null): string {
  return browserPageLabel(url, title);
}

function newTab(url = "about:blank", active = true): HumanBrowserTab {
  return {
    id: `tab-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    url,
    title: tabLabelFromUrl(url),
    favicon: null,
    active,
    historyBack: [],
    historyForward: [],
  };
}

type PersistedSession = {
  tabs: Array<Omit<HumanBrowserTab, "active"> & { active?: boolean }>;
  activeTabId: string;
};

function isValidUrl(url: string): boolean {
  if (!url || url === "about:blank") return true;
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function loadSession(sessionKey: string): HumanBrowserTab[] | null {
  if (typeof localStorage === "undefined") return null;
  try {
    let raw = localStorage.getItem(sessionKey);
    if (!raw && sessionKey === EMBED_SESSION_KEY) {
      raw = localStorage.getItem(LEGACY_SESSION_KEY);
    }
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PersistedSession;
    if (!parsed?.tabs?.length || !parsed.activeTabId) return null;

    const tabs = parsed.tabs
      .slice(0, MAX_TABS)
      .filter((tab) => typeof tab.id === "string" && isValidUrl(tab.url))
      .map((tab) => ({
        id: tab.id,
        url: tab.url,
        title: tab.title?.trim() || tabLabelFromUrl(tab.url),
        favicon: tab.favicon ?? null,
        active: tab.id === parsed.activeTabId,
        historyBack: Array.isArray(tab.historyBack) ? tab.historyBack.filter(isValidUrl) : [],
        historyForward: Array.isArray(tab.historyForward)
          ? tab.historyForward.filter(isValidUrl)
          : [],
      }));

    if (tabs.length === 0) return null;
    if (!tabs.some((tab) => tab.active)) {
      tabs[tabs.length - 1]!.active = true;
    }
    const onlyBlank = tabs.every((tab) => tab.url === "about:blank");
    if (onlyBlank && tabs.length === 1) return null;
    return tabs;
  } catch {
    return null;
  }
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;

function schedulePersist(sessionKey: string, tabs: HumanBrowserTab[]) {
  if (typeof localStorage === "undefined") return;
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    persistTimer = null;
    const active = tabs.find((tab) => tab.active);
    if (!active) return;
    const payload: PersistedSession = {
      tabs: tabs.map(({ active: _active, ...tab }) => tab),
      activeTabId: active.id,
    };
    localStorage.setItem(sessionKey, JSON.stringify(payload));
  }, 200);
}

export class HumanBrowserStore {
  private readonly sessionKey: string;

  tabs = $state<HumanBrowserTab[]>([]);
  urlDraft = $state("");
  loading = $state(false);
  nativeCanGoBack = $state(false);
  nativeCanGoForward = $state(false);
  findOpen = $state(false);
  closedTabs = $state<HumanBrowserTab[]>([]);

  activeTab = $derived(this.tabs.find((tab) => tab.active) ?? this.tabs[0] ?? null);
  activeUrl = $derived(this.activeTab?.url ?? "about:blank");
  showStartPage = $derived(this.activeUrl === "about:blank");
  canGoBack = $derived(
    (this.activeTab?.historyBack.length ?? 0) > 0 || this.nativeCanGoBack,
  );
  canGoForward = $derived(
    (this.activeTab?.historyForward.length ?? 0) > 0 || this.nativeCanGoForward,
  );

  scopeLabel = $derived.by(() => {
    const tab = this.activeTab;
    if (!tab) return "Web";
    return tab.title?.trim() || tabLabelFromUrl(tab.url);
  });

  constructor(sessionKey: string) {
    this.sessionKey = sessionKey;
    this.tabs = loadSession(sessionKey) ?? [newTab()];
    const active = this.activeTab;
    if (active) this.urlDraft = active.url === "about:blank" ? "" : active.url;
  }

  private persist() {
    schedulePersist(this.sessionKey, this.tabs);
  }

  setLoading(loading: boolean) {
    this.loading = loading;
  }

  setNativeNavState(canGoBack: boolean, canGoForward: boolean) {
    this.nativeCanGoBack = canGoBack;
    this.nativeCanGoForward = canGoForward;
  }

  async refreshNativeNavState() {
    try {
      const state = await humanBrowserQueryNavState();
      this.setNativeNavState(state.canGoBack, state.canGoForward);
    } catch {
      // iframe / stub platforms
    }
  }

  private updateActiveTab(
    updater: (tab: HumanBrowserTab) => HumanBrowserTab,
  ): HumanBrowserTab | null {
    const activeIdx = this.tabs.findIndex((tab) => tab.active);
    if (activeIdx < 0) return null;
    const nextTab = updater(this.tabs[activeIdx]!);
    this.tabs = this.tabs.map((tab, idx) => (idx === activeIdx ? nextTab : tab));
    return nextTab;
  }

  private setActiveTabLocal(url: string, title?: string, favicon?: string | null) {
    const activeIdx = this.tabs.findIndex((tab) => tab.active);
    if (activeIdx >= 0) {
      this.updateTabAt(activeIdx, url, title, favicon);
    } else {
      this.tabs = [newTab(url)];
      this.urlDraft = url === "about:blank" ? "" : url;
      if (url !== "about:blank") {
        browserHistory.record(url, tabLabelFromUrl(url, title));
      }
      this.persist();
    }
  }

  private updateTabAt(
    idx: number,
    url: string,
    title?: string,
    favicon?: string | null,
  ) {
    const label = title?.trim() || tabLabelFromUrl(url);
    const wasActive = this.tabs[idx]!.active;
    this.tabs = this.tabs.map((tab, i) =>
      i === idx
        ? {
            ...tab,
            url,
            title: label,
            favicon: favicon ?? tab.favicon ?? null,
          }
        : tab,
    );
    if (wasActive) {
      this.urlDraft = url === "about:blank" ? "" : url;
    }
    if (url !== "about:blank") {
      browserHistory.record(url, label);
    }
    this.persist();
  }

  syncFromNative(payload: HumanBrowserNavigatedPayload) {
    const trimmed = payload.url.trim();
    if (!trimmed || trimmed === "about:blank") return;

    const title = payload.title?.trim();
    const favicon = payload.favicon?.trim() || null;
    const tabId = payload.tabId?.trim();

    if (tabId) {
      const idx = this.tabs.findIndex((tab) => tab.id === tabId);
      if (idx < 0) return;
      const tab = this.tabs[idx]!;
      const sameUrl = trimmed === tab.url;

      if (sameUrl) {
        this.updateTabAt(idx, trimmed, title, favicon);
        if (tab.active) void this.refreshNativeNavState();
        return;
      }

      if (tab.active) {
        const previous = tab.url;
        if (previous && previous !== "about:blank" && previous !== trimmed) {
          this.updateActiveTab((entry) => ({
            ...entry,
            historyBack: [...entry.historyBack, previous],
            historyForward: [],
          }));
        }
        void this.refreshNativeNavState();
      }

      this.updateTabAt(idx, trimmed, title, favicon);
      return;
    }

    const sameUrl = trimmed === this.activeUrl;

    if (sameUrl) {
      this.setActiveTabLocal(trimmed, title, favicon);
      void this.refreshNativeNavState();
      return;
    }

    const previous = this.activeUrl;
    if (previous && previous !== "about:blank" && previous !== trimmed) {
      this.updateActiveTab((tab) => ({
        ...tab,
        historyBack: [...tab.historyBack, previous],
        historyForward: [],
      }));
    }

    this.setActiveTabLocal(trimmed, title, favicon);
    void this.refreshNativeNavState();
  }

  async navigate(input: string, options?: { skipHistory?: boolean }) {
    const trimmed = input.trim();
    if (!trimmed) return;

    let normalized: string;
    try {
      normalized = await resolveBrowserDestination(trimmed);
    } catch {
      return;
    }

    const previous = this.activeUrl;

    if (
      !options?.skipHistory &&
      previous &&
      previous !== "about:blank" &&
      previous !== normalized
    ) {
      this.updateActiveTab((tab) => ({
        ...tab,
        historyBack: [...tab.historyBack, previous],
        historyForward: [],
      }));
    }

    this.urlDraft = normalized;
    this.loading = true;
    try {
      await humanBrowserNavigate(normalized);
      this.setActiveTabLocal(normalized);
    } catch {
      this.loading = false;
    }
  }

  async openTab(url = "about:blank") {
    if (this.tabs.length >= MAX_TABS) return;
    const next = this.tabs.map((tab) => ({ ...tab, active: false }));
    const tab = newTab(url, true);
    next.push(tab);
    this.tabs = next;
    this.urlDraft = url === "about:blank" ? "" : url;
    this.persist();
    this.loading = url !== "about:blank";
    await humanBrowserActivateTab(tab.id, url);
    if (url === "about:blank") this.loading = false;
  }

  async activateTab(tabId: string) {
    const target = this.tabs.find((tab) => tab.id === tabId);
    if (!target || target.active) return;
    this.tabs = this.tabs.map((tab) => ({ ...tab, active: tab.id === tabId }));
    this.urlDraft = target.url === "about:blank" ? "" : target.url;
    this.persist();
    await humanBrowserActivateTab(tabId, target.url);
    void this.refreshNativeNavState();
  }

  async closeTab(tabId: string) {
    const closing = this.tabs.find((tab) => tab.id === tabId);
    const wasActive = closing?.active ?? false;
    if (closing) {
      this.closedTabs = [closing, ...this.closedTabs].slice(0, MAX_CLOSED_TABS);
    }
    await humanBrowserCloseTab(tabId);
    let remaining = this.tabs.filter((tab) => tab.id !== tabId);
    if (remaining.length === 0) {
      remaining = [newTab()];
    } else if (wasActive) {
      remaining = remaining.map((tab, idx) => ({
        ...tab,
        active: idx === remaining.length - 1,
      }));
    }
    this.tabs = remaining;
    this.persist();
    const active = this.activeTab;
    if (active) {
      this.urlDraft = active.url === "about:blank" ? "" : active.url;
      if (wasActive) {
        await humanBrowserActivateTab(active.id, active.url);
      }
    }
    void this.refreshNativeNavState();
  }

  async reopenClosedTab() {
    const tab = this.closedTabs[0];
    if (!tab) return;
    this.closedTabs = this.closedTabs.slice(1);
    if (this.tabs.length >= MAX_TABS) {
      await this.navigate(tab.url, { skipHistory: true });
      return;
    }
    const next = this.tabs.map((t) => ({ ...t, active: false }));
    next.push({ ...tab, active: true, id: newTab(tab.url).id });
    this.tabs = next;
    this.urlDraft = tab.url === "about:blank" ? "" : tab.url;
    this.persist();
    await this.navigate(tab.url, { skipHistory: true });
  }

  async reload() {
    this.loading = true;
    try {
      await humanBrowserReload();
    } catch {
      this.loading = false;
    }
  }

  async stop() {
    await humanBrowserStop();
    this.loading = false;
  }

  async goBack() {
    const active = this.activeTab;
    const previous = active?.historyBack.at(-1);
    if (previous) {
      this.updateActiveTab((tab) => ({
        ...tab,
        historyBack: tab.historyBack.slice(0, -1),
        historyForward:
          tab.url && tab.url !== "about:blank"
            ? [...tab.historyForward, tab.url]
            : tab.historyForward,
      }));
      await this.navigate(previous, { skipHistory: true });
      return;
    }
    this.loading = true;
    await humanBrowserGoBack();
    void this.refreshNativeNavState();
  }

  async goForward() {
    const active = this.activeTab;
    const next = active?.historyForward.at(-1);
    if (next) {
      this.updateActiveTab((tab) => ({
        ...tab,
        historyForward: tab.historyForward.slice(0, -1),
        historyBack:
          tab.url && tab.url !== "about:blank"
            ? [...tab.historyBack, tab.url]
            : tab.historyBack,
      }));
      await this.navigate(next, { skipHistory: true });
      return;
    }
    this.loading = true;
    await humanBrowserGoForward();
    void this.refreshNativeNavState();
  }

  async findInPage(query: string, forward = true) {
    return humanBrowserFindInPage(query, forward);
  }

  openFindBar() {
    this.findOpen = true;
  }

  closeFindBar() {
    this.findOpen = false;
  }
}

export const humanBrowserEmbed = new HumanBrowserStore(EMBED_SESSION_KEY);
export const humanBrowserPopout = new HumanBrowserStore(POPOUT_SESSION_KEY);
/** Main shell embed — default for agent/chat integrations. */
export const humanBrowser = humanBrowserEmbed;
