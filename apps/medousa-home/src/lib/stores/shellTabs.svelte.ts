/**
 * Shell-level tab host — center strip for chat / LME / web / singleton surfaces.
 * Shaped for a future EditorGroup[] split-view sprint (single group today).
 */

import { chat } from "$lib/stores/chat.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import {
  isShellSurfaceTabId,
  type EditorGroup,
  type ShellTab,
} from "$lib/types/shellTabs";
import type { Surface } from "$lib/types/ui";
import { tabDisplayLabel } from "$lib/utils/browserFavicon";
import { formatSessionLabel } from "$lib/utils/formatSession";

const MAX_TABS = 16;
const MAIN_GROUP_ID = "main";
const PERSIST_KEY = "medousa-home-shell-tabs-v1";

type PersistedShellTabs = {
  tabs: ShellTab[];
  group: EditorGroup;
};

function newTabId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

function surfaceTitle(surfaceId: string): string {
  switch (surfaceId) {
    case "library":
    case "automations":
      return "Workspace";
    case "chat":
      return "Chat";
    case "peers":
      return "Peers";
    case "messaging":
      return "Channels";
    case "context":
      return "Context";
    case "work":
      return "Work";
    case "calendar":
      return "Calendar";
    case "settings":
      return "Settings";
    case "runtime":
      return "Runtime";
    case "profiles":
      return "Profiles";
    case "web":
      return "Web";
    default:
      return surfaceId;
  }
}

function focusSurfaceHint(tab: ShellTab | null): string | null {
  if (!tab) return null;
  if (tab.kind === "chat") return "chat";
  if (tab.kind === "lme") return "library";
  if (tab.kind === "web") return "web";
  return tab.surfaceId;
}

function loadPersisted(): PersistedShellTabs | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(PERSIST_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PersistedShellTabs;
    if (!parsed?.tabs || !parsed?.group) return null;
    if (!Array.isArray(parsed.tabs) || !Array.isArray(parsed.group.tabIds)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function schedulePersist(tabs: ShellTab[], group: EditorGroup) {
  if (typeof localStorage === "undefined") return;
  try {
    const payload: PersistedShellTabs = { tabs, group };
    localStorage.setItem(PERSIST_KEY, JSON.stringify(payload));
  } catch {
    /* ignore quota */
  }
}

export class ShellTabsStore {
  tabs = $state<ShellTab[]>([]);
  groups = $state<EditorGroup[]>([
    { id: MAIN_GROUP_ID, tabIds: [], activeTabId: null },
  ]);
  private bootstrapped = false;
  /** Prevents LME/browser → shell mirror loops while shell is driving activate. */
  private suppressMirrorDepth = 0;

  private get suppressMirror() {
    return this.suppressMirrorDepth > 0;
  }

  private beginSuppressMirror() {
    this.suppressMirrorDepth += 1;
  }

  private endSuppressMirror() {
    this.suppressMirrorDepth = Math.max(0, this.suppressMirrorDepth - 1);
  }

  mainGroup = $derived(this.groups[0]!);

  activeTabId = $derived(this.mainGroup.activeTabId);

  activeTab = $derived.by(() => {
    const id = this.activeTabId;
    if (!id) return null;
    return this.tabs.find((tab) => tab.id === id) ?? null;
  });

  orderedTabs = $derived.by(() => {
    const byId = new Map(this.tabs.map((tab) => [tab.id, tab]));
    return this.mainGroup.tabIds
      .map((id) => byId.get(id))
      .filter((tab): tab is ShellTab => Boolean(tab));
  });

  private persist() {
    schedulePersist(this.tabs, this.mainGroup);
  }

  private setActiveInGroup(tabId: string | null) {
    this.groups = this.groups.map((group, index) =>
      index === 0 ? { ...group, activeTabId: tabId } : group,
    );
  }

  private setGroupTabIds(tabIds: string[], activeTabId: string | null) {
    this.groups = [
      { id: MAIN_GROUP_ID, tabIds, activeTabId },
      ...this.groups.slice(1),
    ];
  }

  private syncLayoutHint(tab: ShellTab | null) {
    const surface = focusSurfaceHint(tab);
    if (!surface) return;
    layout.focusDesktopSurface(surface);
  }

  private enforceCap(preferKeepId?: string) {
    while (this.tabs.length > MAX_TABS) {
      const drop =
        this.mainGroup.tabIds.find((id) => id !== preferKeepId && id !== this.activeTabId) ??
        this.mainGroup.tabIds[0];
      if (!drop) break;
      this.removeTabInternal(drop);
    }
  }

  private removeTabInternal(tabId: string) {
    this.tabs = this.tabs.filter((tab) => tab.id !== tabId);
    const tabIds = this.mainGroup.tabIds.filter((id) => id !== tabId);
    let active = this.mainGroup.activeTabId;
    if (active === tabId) {
      active = tabIds[tabIds.length - 1] ?? null;
    }
    this.setGroupTabIds(tabIds, active);
  }

  private insertTab(tab: ShellTab, activate: boolean) {
    this.tabs = [...this.tabs, tab];
    const tabIds = [...this.mainGroup.tabIds, tab.id];
    const activeTabId = activate ? tab.id : this.mainGroup.activeTabId;
    this.setGroupTabIds(tabIds, activeTabId);
    this.enforceCap(tab.id);
    if (activate) {
      this.syncLayoutHint(tab);
    }
    this.persist();
  }

  /** Restore persisted tabs or seed from last surface / current session. */
  bootstrap() {
    if (this.bootstrapped) return;
    this.bootstrapped = true;

    const persisted = loadPersisted();
    if (persisted && persisted.tabs.length > 0) {
      this.tabs = persisted.tabs;
      this.groups = [persisted.group];
      const active = this.activeTab;
      if (active) {
        void this.activate(active.id, { skipOpen: true });
      }
      return;
    }

    const surface = layout.desktopSurface;
    if (surface === "web") {
      const browserTab = humanBrowser.activeTab;
      if (browserTab) {
        this.openWeb(browserTab.id, { activate: true });
        return;
      }
    }
    if (surface === "library" || surface === "automations") {
      const lme = lmeWorkspace.activeTab;
      if (lme) {
        this.openLme(lme.tabId, { activate: true });
        return;
      }
      this.openSurface("library", { activate: true });
      return;
    }
    if (isShellSurfaceTabId(surface) && surface !== "library") {
      this.openSurface(surface as Surface, { activate: true });
      return;
    }

    const sessionId = chat.sessionId?.trim();
    if (sessionId) {
      this.openChat(sessionId, { activate: true });
      return;
    }
    this.openSurface("library", { activate: true });
  }

  openChat(
    sessionId: string,
    options?: { activate?: boolean; title?: string },
  ): string | null {
    const trimmed = sessionId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const existing = this.tabs.find(
      (tab) => tab.kind === "chat" && tab.sessionId === trimmed,
    );
    if (existing) {
      if (options?.title) {
        this.patchTitle(existing.id, options.title);
      }
      if (activate) void this.activate(existing.id);
      return existing.id;
    }
    const session = chat.sessions.find((row) => row.session_id === trimmed);
    const title =
      options?.title?.trim() ||
      (session ? formatSessionLabel(session) : null) ||
      "Chat";
    const tab: ShellTab = {
      id: newTabId("chat"),
      kind: "chat",
      sessionId: trimmed,
      title,
    };
    this.insertTab(tab, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openLme(
    lmeTabId: string,
    options?: { activate?: boolean; title?: string },
  ): string | null {
    const trimmed = lmeTabId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const existing = this.tabs.find(
      (tab) => tab.kind === "lme" && tab.lmeTabId === trimmed,
    );
    const lmeTab = lmeWorkspace.tabs.find((tab) => tab.tabId === trimmed);
    const title =
      options?.title?.trim() || lmeTab?.title?.trim() || "Document";
    if (existing) {
      this.patchTitle(existing.id, title);
      if (activate) void this.activate(existing.id);
      return existing.id;
    }
    // Drop singleton Workspace surface tab when a real doc opens.
    const librarySurface = this.tabs.find(
      (tab) => tab.kind === "surface" && tab.surfaceId === "library",
    );
    if (librarySurface) {
      this.removeTabInternal(librarySurface.id);
    }
    const tab: ShellTab = {
      id: newTabId("lme"),
      kind: "lme",
      lmeTabId: trimmed,
      title,
    };
    this.insertTab(tab, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openWeb(
    browserTabId: string,
    options?: { activate?: boolean; title?: string },
  ): string | null {
    const trimmed = browserTabId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const existing = this.tabs.find(
      (tab) => tab.kind === "web" && tab.browserTabId === trimmed,
    );
    const browserTab = humanBrowser.tabs.find((tab) => tab.id === trimmed);
    const title =
      options?.title?.trim() ||
      (browserTab ? tabDisplayLabel(browserTab.title, browserTab.url) : "Web");
    if (existing) {
      this.patchTitle(existing.id, title);
      if (activate) void this.activate(existing.id);
      return existing.id;
    }
    const tab: ShellTab = {
      id: newTabId("web"),
      kind: "web",
      browserTabId: trimmed,
      title,
    };
    this.insertTab(tab, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openSurface(
    surfaceId: string,
    options?: { activate?: boolean },
  ): string | null {
    let next = surfaceId === "home" ? "chat" : surfaceId;
    if (next === "automations" || next === "workshop") next = "library";
    if (next === "chat") {
      const sessionId = chat.sessionId?.trim();
      if (sessionId) return this.openChat(sessionId, { activate: options?.activate !== false });
    }
    if (next === "web") {
      const browserTab = humanBrowser.activeTab;
      if (browserTab) {
        return this.openWeb(browserTab.id, { activate: options?.activate !== false });
      }
      void humanBrowser.openTab("about:blank").then(() => {
        const created = humanBrowser.activeTab;
        if (created) this.openWeb(created.id, { activate: true });
      });
      return null;
    }
    if (!isShellSurfaceTabId(next)) {
      // Unknown / custom — still open as surface tab.
    }
    const activate = options?.activate !== false;
    const existing = this.tabs.find(
      (tab) => tab.kind === "surface" && tab.surfaceId === next,
    );
    if (existing) {
      if (activate) void this.activate(existing.id);
      return existing.id;
    }
    const tab: ShellTab = {
      id: newTabId("surface"),
      kind: "surface",
      surfaceId: next as Surface,
      title: surfaceTitle(next),
    };
    this.insertTab(tab, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  /** Rail / nest entry point — open the right tab kind for a destination. */
  openDestination(surfaceId: string) {
    this.openSurface(surfaceId, { activate: true });
  }

  async activate(tabId: string, options?: { skipOpen?: boolean }) {
    const tab = this.tabs.find((entry) => entry.id === tabId);
    if (!tab) return;
    this.setActiveInGroup(tabId);
    this.syncLayoutHint(tab);
    this.persist();

    if (options?.skipOpen) {
      // Titles / restore path — still hydrate backing stores below.
    }

    this.beginSuppressMirror();
    try {
      if (tab.kind === "chat") {
        if (chat.sessionId !== tab.sessionId) {
          await chat.switchSession(tab.sessionId);
        }
        return;
      }
      if (tab.kind === "lme") {
        if (lmeWorkspace.activeTabId !== tab.lmeTabId) {
          await lmeWorkspace.activateTab(tab.lmeTabId);
        }
        return;
      }
      if (tab.kind === "web") {
        if (humanBrowser.activeTab?.id !== tab.browserTabId) {
          await humanBrowser.activateTab(tab.browserTabId);
        }
      }
    } finally {
      this.endSuppressMirror();
    }
  }

  /** Called from LME open/activate paths so docs appear in the shell strip. */
  mirrorLmeTab(lmeTabId: string, options?: { activate?: boolean; title?: string }) {
    if (this.suppressMirror) return;
    this.openLme(lmeTabId, {
      activate: options?.activate !== false,
      title: options?.title,
    });
  }

  /** Called from human-browser open/activate paths. */
  mirrorWebTab(browserTabId: string, options?: { activate?: boolean; title?: string }) {
    if (this.suppressMirror) return;
    this.openWeb(browserTabId, {
      activate: options?.activate !== false,
      title: options?.title,
    });
  }

  close(tabId: string) {
    const tab = this.tabs.find((entry) => entry.id === tabId);
    if (!tab) return;
    const wasActive = this.activeTabId === tabId;
    this.removeTabInternal(tabId);

    // Close backing LME/browser tab when shell tab closes (chat session stays).
    this.beginSuppressMirror();
    try {
      if (tab.kind === "lme") {
        void lmeWorkspace.closeTab(tab.lmeTabId, { activateNext: false });
      } else if (tab.kind === "web") {
        void humanBrowser.closeTab(tab.browserTabId);
      }
    } finally {
      this.endSuppressMirror();
    }

    if (wasActive && this.activeTabId) {
      void this.activate(this.activeTabId);
    } else if (!this.activeTabId) {
      // Empty host — seed a chat or workspace tab.
      const sessionId = chat.sessionId?.trim();
      if (sessionId) {
        this.openChat(sessionId, { activate: true });
      } else {
        this.openSurface("library", { activate: true });
      }
    }
    this.persist();
  }

  patchTitle(tabId: string, title: string) {
    const trimmed = title.trim();
    if (!trimmed) return;
    this.tabs = this.tabs.map((tab) =>
      tab.id === tabId ? { ...tab, title: trimmed } : tab,
    );
    this.persist();
  }

  /** Keep shell strip labels in sync with backing stores. */
  syncTitlesFromStores() {
    let changed = false;
    const next = this.tabs.map((tab) => {
      if (tab.kind === "chat") {
        const session = chat.sessions.find((row) => row.session_id === tab.sessionId);
        if (!session) return tab;
        const title = formatSessionLabel(session);
        if (title !== tab.title) {
          changed = true;
          return { ...tab, title };
        }
        return tab;
      }
      if (tab.kind === "lme") {
        const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
        if (!lme) return tab;
        const title = lme.title?.trim() || tab.title;
        if (title !== tab.title) {
          changed = true;
          return { ...tab, title };
        }
        return tab;
      }
      if (tab.kind === "web") {
        const browserTab = humanBrowser.tabs.find((entry) => entry.id === tab.browserTabId);
        if (!browserTab) return tab;
        const title = tabDisplayLabel(browserTab.title, browserTab.url);
        if (title !== tab.title) {
          changed = true;
          return { ...tab, title };
        }
      }
      return tab;
    });
    if (changed) {
      this.tabs = next;
      this.persist();
    }
  }

  /** Ensure every LME doc has a shell tab (and drop orphans). Does not steal focus. */
  syncFromLmeWorkspace() {
    const lmeIds = new Set(lmeWorkspace.tabs.map((tab) => tab.tabId));
    for (const lme of lmeWorkspace.tabs) {
      const existing = this.tabs.find(
        (tab) => tab.kind === "lme" && tab.lmeTabId === lme.tabId,
      );
      if (!existing) {
        this.openLme(lme.tabId, { activate: false, title: lme.title });
      } else if (existing.title !== (lme.title?.trim() || existing.title)) {
        this.patchTitle(existing.id, lme.title);
      }
    }
    for (const tab of [...this.tabs]) {
      if (tab.kind === "lme" && !lmeIds.has(tab.lmeTabId)) {
        this.removeTabInternal(tab.id);
      }
    }
    this.persist();
  }

  /** Ensure every browser page has a shell tab. Does not steal focus. */
  syncFromHumanBrowser() {
    const browserIds = new Set(humanBrowser.tabs.map((tab) => tab.id));
    const hasWebShell = this.tabs.some((tab) => tab.kind === "web");
    const webEngaged =
      hasWebShell ||
      this.activeTab?.kind === "web" ||
      layout.desktopSurface === "web";

    if (webEngaged) {
      for (const browserTab of humanBrowser.tabs) {
        const existing = this.tabs.find(
          (tab) => tab.kind === "web" && tab.browserTabId === browserTab.id,
        );
        const title = tabDisplayLabel(browserTab.title, browserTab.url);
        if (!existing) {
          this.openWeb(browserTab.id, { activate: false, title });
        } else if (existing.title !== title) {
          this.patchTitle(existing.id, title);
        }
      }
    }

    for (const tab of [...this.tabs]) {
      if (tab.kind === "web" && !browserIds.has(tab.browserTabId)) {
        this.removeTabInternal(tab.id);
      }
    }
    this.persist();
  }

  /** Whether a tab body should stay mounted while inactive. */
  shouldKeepAlive(tab: ShellTab): boolean {
    if (tab.kind === "chat" || tab.kind === "web") return true;
    if (tab.kind === "lme") {
      // Keep LME host alive whenever any LME tab exists (shared editor host).
      return true;
    }
    return false;
  }
}

export const shellTabs = new ShellTabsStore();
