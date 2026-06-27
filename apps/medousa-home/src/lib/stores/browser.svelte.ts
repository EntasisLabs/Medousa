/** Shared browser workspace state — tabs, control handoff, active URL. */

import type { BrowserControl, BrowserTab, TabGroup, TabOpenedBy } from "$lib/browserBridge";
import {
  bridgeActivateTab,
  bridgeCloseTab,
  bridgeCreateTabGroup,
  bridgeGetTabGroup,
  bridgeLinkWorkCard,
  bridgeNavigate,
  bridgeOpenTab,
  bridgeSetControl,
  isLocalTabGroupId,
} from "$lib/browserBridge";

const TAB_GROUP_KEY = "medousa-browser-tab-group-id";

function loadTabGroupId(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(TAB_GROUP_KEY);
}

function saveTabGroupId(id: string | null) {
  if (typeof localStorage === "undefined") return;
  if (id) localStorage.setItem(TAB_GROUP_KEY, id);
  else localStorage.removeItem(TAB_GROUP_KEY);
}

function localTab(url: string, openedBy: TabOpenedBy, title?: string): BrowserTab {
  const id = `tab-local-${Date.now()}`;
  return {
    id,
    url,
    title: title?.trim() || tabLabelFromUrl(url),
    favicon: null,
    opened_by: openedBy,
    active: true,
  };
}

function tabLabelFromUrl(url: string): string {
  try {
    return new URL(url).hostname || url;
  } catch {
    return url;
  }
}

export class BrowserStore {
  tabGroupId = $state<string | null>(loadTabGroupId());
  tabs = $state<BrowserTab[]>([]);
  control = $state<BrowserControl>("user");
  scopedSessionId = $state<string | null>(null);
  workCardId = $state<string | null>(null);
  urlDraft = $state("");
  loading = $state(false);
  private historyBack = $state<string[]>([]);
  private historyForward = $state<string[]>([]);

  activeTab = $derived(this.tabs.find((tab) => tab.active) ?? this.tabs[0] ?? null);

  activeUrl = $derived(this.activeTab?.url ?? "about:blank");

  canGoBack = $derived(this.historyBack.length > 0);
  canGoForward = $derived(this.historyForward.length > 0);

  scopeLabel = $derived.by(() => {
    const url = this.activeUrl;
    if (!url || url === "about:blank") return "Web";
    try {
      return new URL(url).hostname;
    } catch {
      return url.slice(0, 48);
    }
  });

  async ensureTabGroup(chatSessionId?: string | null): Promise<string> {
    if (this.tabGroupId && isLocalTabGroupId(this.tabGroupId)) {
      try {
        const group = await bridgeCreateTabGroup({
          chatSessionId: chatSessionId ?? undefined,
          workCardId: this.workCardId ?? undefined,
        });
        const priorTabs = this.tabs;
        this.applyGroup(group);
        if (priorTabs.length > 0 && group.tabs.length === 0) {
          this.tabs = priorTabs;
          const active = this.activeTab;
          if (active) this.urlDraft = active.url;
        }
        return group.id;
      } catch {
        return this.tabGroupId;
      }
    }
    if (this.tabGroupId) {
      if (!isLocalTabGroupId(this.tabGroupId)) {
        try {
          await this.refreshFromBridge();
        } catch {
          // Bridge unavailable — keep local state.
        }
      }
      return this.tabGroupId;
    }
    try {
      const group = await bridgeCreateTabGroup({
        chatSessionId: chatSessionId ?? undefined,
        workCardId: this.workCardId ?? undefined,
      });
      this.applyGroup(group);
      return group.id;
    } catch {
      const localId = `tg-local-${Date.now()}`;
      this.tabGroupId = localId;
      saveTabGroupId(localId);
      if (this.tabs.length === 0) {
        this.tabs = [localTab("about:blank", "user", "New tab")];
      }
      return localId;
    }
  }

  applyGroup(group: TabGroup) {
    this.tabGroupId = group.id;
    saveTabGroupId(group.id);
    this.tabs = group.tabs;
    this.control = group.control;
    if (group.chat_session_id) this.scopedSessionId = group.chat_session_id;
    if (group.work_card_id) this.workCardId = group.work_card_id;
    const active = this.activeTab;
    if (active) this.urlDraft = active.url;
  }

  async refreshFromBridge() {
    if (!this.tabGroupId) return;
    const group = await bridgeGetTabGroup(this.tabGroupId);
    if (group) this.applyGroup(group);
  }

  private updateActiveTabLocal(url: string, openedBy: TabOpenedBy, title?: string) {
    const label = title?.trim() || tabLabelFromUrl(url);
    const activeIdx = this.tabs.findIndex((tab) => tab.active);
    if (activeIdx >= 0) {
      this.tabs = this.tabs.map((tab, idx) =>
        idx === activeIdx ? { ...tab, url, title: label, opened_by: openedBy } : tab,
      );
    } else {
      this.tabs = [localTab(url, openedBy, label)];
    }
    this.urlDraft = url;
  }

  async syncFromNative(url: string) {
    const trimmed = url.trim();
    if (!trimmed || trimmed === "about:blank" || trimmed === this.activeUrl) return;

    const previous = this.activeUrl;
    if (previous && previous !== "about:blank" && previous !== trimmed) {
      this.historyBack = [...this.historyBack, previous];
      this.historyForward = [];
    }

    await this.ensureTabGroup(this.scopedSessionId);
    if (this.tabGroupId && !isLocalTabGroupId(this.tabGroupId)) {
      try {
        const group = await bridgeNavigate(this.tabGroupId, trimmed, "user");
        if (group) {
          this.applyGroup(group);
          return;
        }
      } catch {
        // local fallback below
      }
    }

    this.updateActiveTabLocal(trimmed, "user");
  }

  async navigate(
    url: string,
    openedBy: TabOpenedBy = "user",
    title?: string,
    options?: { skipHistory?: boolean },
  ) {
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
    await this.ensureTabGroup(this.scopedSessionId);
    if (!this.tabGroupId) return;

    this.loading = true;
    try {
      const group = await bridgeNavigate(this.tabGroupId, normalized, openedBy, title);
      if (group) {
        this.applyGroup(group);
        return;
      }
    } catch {
      // Fall back to local-only tabs when BrowserHost is offline.
    } finally {
      this.loading = false;
    }

    this.updateActiveTabLocal(normalized, openedBy, title);
  }

  async openTab(url = "about:blank", openedBy: TabOpenedBy = "user") {
    await this.ensureTabGroup(this.scopedSessionId);
    if (!this.tabGroupId) return;
    try {
      const group = await bridgeOpenTab(this.tabGroupId, url, openedBy);
      if (group) {
        this.applyGroup(group);
        return;
      }
    } catch {
      // local fallback
    }
    const next = this.tabs.map((tab) => ({ ...tab, active: false }));
    next.push(localTab(url, openedBy));
    this.tabs = next;
    this.urlDraft = url;
  }

  async activateTab(tabId: string) {
    if (!this.tabGroupId) return;
    try {
      const group = await bridgeActivateTab(this.tabGroupId, tabId);
      if (group) {
        this.applyGroup(group);
        return;
      }
    } catch {
      // local fallback
    }
    this.tabs = this.tabs.map((tab) => ({
      ...tab,
      active: tab.id === tabId,
    }));
    const active = this.activeTab;
    if (active) this.urlDraft = active.url;
  }

  async closeTab(tabId: string) {
    if (!this.tabGroupId) return;
    try {
      const group = await bridgeCloseTab(this.tabGroupId, tabId);
      if (group) {
        this.applyGroup(group);
        return;
      }
    } catch {
      // local fallback
    }
    const closing = this.tabs.find((tab) => tab.id === tabId);
    const wasActive = closing?.active ?? false;
    const remaining = this.tabs.filter((tab) => tab.id !== tabId);
    if (remaining.length === 0) {
      remaining.push(localTab("about:blank", "user", "New tab"));
    } else if (wasActive) {
      remaining[remaining.length - 1]!.active = true;
    }
    this.tabs = remaining;
    const active = this.activeTab;
    if (active) this.urlDraft = active.url;
  }

  async setControl(control: BrowserControl) {
    this.control = control;
    if (!this.tabGroupId) return;
    try {
      const group = await bridgeSetControl(this.tabGroupId, control);
      if (group) this.applyGroup(group);
    } catch {
      // local-only control state
    }
  }

  takeControl() {
    void this.setControl("user");
  }

  handBackToAgent() {
    void this.setControl("agent");
  }

  handleAgentNavigation(url: string, title?: string) {
    void this.navigate(url, "agent", title);
    if (this.control === "user") return;
    void this.setControl("agent");
  }

  linkSession(chatSessionId: string | null) {
    this.scopedSessionId = chatSessionId;
  }

  async linkWorkCard(workCardId: string | null) {
    const trimmed = workCardId?.trim() || null;
    this.workCardId = trimmed;
    if (!this.tabGroupId) return;
    try {
      const group = await bridgeLinkWorkCard(this.tabGroupId, trimmed);
      if (group) this.applyGroup(group);
    } catch {
      // local-only association
    }
  }

  async goBack() {
    const previous = this.historyBack.at(-1);
    if (!previous) return;
    this.historyBack = this.historyBack.slice(0, -1);
    if (this.activeUrl && this.activeUrl !== "about:blank") {
      this.historyForward = [...this.historyForward, this.activeUrl];
    }
    await this.navigate(previous, "user", undefined, { skipHistory: true });
  }

  async goForward() {
    const next = this.historyForward.at(-1);
    if (!next) return;
    this.historyForward = this.historyForward.slice(0, -1);
    if (this.activeUrl && this.activeUrl !== "about:blank") {
      this.historyBack = [...this.historyBack, this.activeUrl];
    }
    await this.navigate(next, "user", undefined, { skipHistory: true });
  }
}

export const browser = new BrowserStore();
