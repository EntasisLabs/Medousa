/** Local-only browser state for the human-first desktop browser window. */

import {
  humanBrowserGoBack,
  humanBrowserGoForward,
  humanBrowserNavigate,
  humanBrowserReload,
  type HumanBrowserNavigatedPayload,
} from "$lib/humanBrowser";
import { browserPageLabel } from "$lib/utils/browserUrl";

export type HumanBrowserTab = {
  id: string;
  url: string;
  title: string;
  active: boolean;
  historyBack: string[];
  historyForward: string[];
};

const SESSION_KEY = "medousa-browser-session";
const MAX_TABS = 8;

function tabLabelFromUrl(url: string): string {
  return browserPageLabel(url);
}

function newTab(url = "about:blank", active = true): HumanBrowserTab {
  return {
    id: `tab-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    url,
    title: tabLabelFromUrl(url),
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

function loadSession(): HumanBrowserTab[] | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(SESSION_KEY);
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

function schedulePersist(tabs: HumanBrowserTab[]) {
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
    localStorage.setItem(SESSION_KEY, JSON.stringify(payload));
  }, 200);
}

export class HumanBrowserStore {
  tabs = $state<HumanBrowserTab[]>(loadSession() ?? [newTab()]);
  urlDraft = $state("");
  loading = $state(false);

  activeTab = $derived(this.tabs.find((tab) => tab.active) ?? this.tabs[0] ?? null);
  activeUrl = $derived(this.activeTab?.url ?? "about:blank");
  canGoBack = $derived((this.activeTab?.historyBack.length ?? 0) > 0);
  canGoForward = $derived((this.activeTab?.historyForward.length ?? 0) > 0);

  scopeLabel = $derived.by(() => {
    const tab = this.activeTab;
    if (!tab) return "Web";
    return tab.title?.trim() || tabLabelFromUrl(tab.url);
  });

  constructor() {
    const active = this.activeTab;
    if (active) this.urlDraft = active.url === "about:blank" ? "" : active.url;
  }

  private persist() {
    schedulePersist(this.tabs);
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

  private setActiveTabLocal(url: string, title?: string) {
    const label = title?.trim() || tabLabelFromUrl(url);
    const activeIdx = this.tabs.findIndex((tab) => tab.active);
    if (activeIdx >= 0) {
      this.tabs = this.tabs.map((tab, idx) =>
        idx === activeIdx ? { ...tab, url, title: label } : tab,
      );
    } else {
      this.tabs = [newTab(url)];
    }
    this.urlDraft = url === "about:blank" ? "" : url;
    this.persist();
  }

  syncFromNative(payload: HumanBrowserNavigatedPayload) {
    const trimmed = payload.url.trim();
    if (!trimmed) return;

    const title = payload.title?.trim();
    const sameUrl = trimmed === this.activeUrl;

    if (sameUrl && title) {
      this.setActiveTabLocal(trimmed, title);
      return;
    }
    if (sameUrl) return;

    const previous = this.activeUrl;
    if (previous && previous !== "about:blank" && previous !== trimmed) {
      this.updateActiveTab((tab) => ({
        ...tab,
        historyBack: [...tab.historyBack, previous],
        historyForward: [],
      }));
    }

    this.setActiveTabLocal(trimmed, title ?? undefined);
  }

  async navigate(url: string, options?: { skipHistory?: boolean }) {
    const trimmed = url.trim();
    if (!trimmed) return;
    const normalized = trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;
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
    } finally {
      this.loading = false;
    }
  }

  async openTab(url = "about:blank") {
    const next = this.tabs.map((tab) => ({ ...tab, active: false }));
    const tab = newTab(url, true);
    next.push(tab);
    this.tabs = next;
    this.urlDraft = url === "about:blank" ? "" : url;
    this.persist();
    if (url !== "about:blank") {
      await this.navigate(url, { skipHistory: true });
    } else {
      await humanBrowserNavigate("about:blank");
    }
  }

  async activateTab(tabId: string) {
    const target = this.tabs.find((tab) => tab.id === tabId);
    if (!target || target.active) return;
    this.tabs = this.tabs.map((tab) => ({ ...tab, active: tab.id === tabId }));
    this.urlDraft = target.url === "about:blank" ? "" : target.url;
    this.persist();
    await humanBrowserNavigate(target.url);
  }

  async closeTab(tabId: string) {
    const closing = this.tabs.find((tab) => tab.id === tabId);
    const wasActive = closing?.active ?? false;
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
      if (wasActive) await humanBrowserNavigate(active.url);
    }
  }

  async reload() {
    await humanBrowserReload();
  }

  async goBack() {
    const active = this.activeTab;
    const previous = active?.historyBack.at(-1);
    if (!previous) {
      await humanBrowserGoBack();
      return;
    }
    this.updateActiveTab((tab) => ({
      ...tab,
      historyBack: tab.historyBack.slice(0, -1),
      historyForward:
        tab.url && tab.url !== "about:blank"
          ? [...tab.historyForward, tab.url]
          : tab.historyForward,
    }));
    await this.navigate(previous, { skipHistory: true });
  }

  async goForward() {
    const active = this.activeTab;
    const next = active?.historyForward.at(-1);
    if (!next) {
      await humanBrowserGoForward();
      return;
    }
    this.updateActiveTab((tab) => ({
      ...tab,
      historyForward: tab.historyForward.slice(0, -1),
      historyBack:
        tab.url && tab.url !== "about:blank"
          ? [...tab.historyBack, tab.url]
          : tab.historyBack,
    }));
    await this.navigate(next, { skipHistory: true });
  }
}

export const humanBrowser = new HumanBrowserStore();
