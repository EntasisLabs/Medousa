/** Local-only browser state for the human-first desktop browser window. */

import {
  humanBrowserGoBack,
  humanBrowserGoForward,
  humanBrowserNavigate,
  humanBrowserReload,
  type HumanBrowserNavigatedPayload,
} from "$lib/humanBrowser";

export type HumanBrowserTab = {
  id: string;
  url: string;
  title: string;
  active: boolean;
};

function tabLabelFromUrl(url: string): string {
  if (!url || url === "about:blank") return "New tab";
  try {
    return new URL(url).hostname || url;
  } catch {
    return url.slice(0, 48);
  }
}

function newTab(url = "about:blank"): HumanBrowserTab {
  return {
    id: `tab-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    url,
    title: tabLabelFromUrl(url),
    active: true,
  };
}

export class HumanBrowserStore {
  tabs = $state<HumanBrowserTab[]>([newTab()]);
  urlDraft = $state("");
  loading = $state(false);
  private historyBack = $state<string[]>([]);
  private historyForward = $state<string[]>([]);

  activeTab = $derived(this.tabs.find((tab) => tab.active) ?? this.tabs[0] ?? null);
  activeUrl = $derived(this.activeTab?.url ?? "about:blank");
  canGoBack = $derived(this.historyBack.length > 0);
  canGoForward = $derived(this.historyForward.length > 0);

  scopeLabel = $derived.by(() => {
    const tab = this.activeTab;
    if (!tab) return "Web";
    return tab.title?.trim() || tabLabelFromUrl(tab.url);
  });

  constructor() {
    const active = this.activeTab;
    if (active) this.urlDraft = active.url === "about:blank" ? "" : active.url;
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
  }

  syncFromNative(payload: HumanBrowserNavigatedPayload) {
    const trimmed = payload.url.trim();
    if (!trimmed || trimmed === this.activeUrl) return;

    const previous = this.activeUrl;
    if (previous && previous !== "about:blank" && previous !== trimmed) {
      this.historyBack = [...this.historyBack, previous];
      this.historyForward = [];
    }

    this.setActiveTabLocal(trimmed, payload.title ?? undefined);
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
      this.historyBack = [...this.historyBack, previous];
      this.historyForward = [];
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
    const tab = newTab(url);
    next.push(tab);
    this.tabs = next;
    this.urlDraft = url === "about:blank" ? "" : url;
    if (url !== "about:blank") {
      await this.navigate(url, { skipHistory: true });
    } else {
      await humanBrowserNavigate("about:blank");
    }
  }

  async activateTab(tabId: string) {
    const target = this.tabs.find((tab) => tab.id === tabId);
    if (!target) return;
    this.tabs = this.tabs.map((tab) => ({ ...tab, active: tab.id === tabId }));
    this.urlDraft = target.url === "about:blank" ? "" : target.url;
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
    const previous = this.historyBack.at(-1);
    if (!previous) {
      await humanBrowserGoBack();
      return;
    }
    this.historyBack = this.historyBack.slice(0, -1);
    if (this.activeUrl && this.activeUrl !== "about:blank") {
      this.historyForward = [...this.historyForward, this.activeUrl];
    }
    await this.navigate(previous, { skipHistory: true });
  }

  async goForward() {
    const next = this.historyForward.at(-1);
    if (!next) {
      await humanBrowserGoForward();
      return;
    }
    this.historyForward = this.historyForward.slice(0, -1);
    if (this.activeUrl && this.activeUrl !== "about:blank") {
      this.historyBack = [...this.historyBack, this.activeUrl];
    }
    await this.navigate(next, { skipHistory: true });
  }
}

export const humanBrowser = new HumanBrowserStore();
