/**
 * Shell-level tab host + binary split tree (TMUX-style panes).
 */

import { chat } from "$lib/stores/chat.svelte";
import { chatStreamPool } from "$lib/stores/chatStreamPool.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { vault } from "$lib/stores/vault.svelte";
import {
  isShellSurfaceTabId,
  MAX_SHELL_PANES,
  type EditorGroup,
  type ShellDesktop,
  type ShellDesktopLayout,
  type ShellTab,
  type SplitDirection,
  type SplitNode,
} from "$lib/types/shellTabs";
import type { Surface } from "$lib/types/ui";
import { tabDisplayLabel } from "$lib/utils/browserFavicon";
import { formatSessionLabel } from "$lib/utils/formatSession";
import {
  clampRatio,
  collectGroupIds,
  countLeaves,
  leafOrder,
  migrateV1ToSplitRoot,
  neighborInDirection,
  newSplitId,
  removeLeaf,
  setBranchRatio,
  splitLeaf,
  type FocusDir,
} from "$lib/utils/shellSplitTree";

const MAX_TABS = 16;
const MAIN_GROUP_ID = "main";
const DEFAULT_DESKTOP_NAME = "Main";
const PERSIST_KEY_V1 = "medousa-home-shell-tabs-v1";
const PERSIST_KEY_V2 = "medousa-home-shell-tabs-v2";
const PERSIST_KEY = "medousa-home-shell-tabs-v3";

type PersistedV2 = ShellDesktopLayout;

type PersistedV3 = {
  desktops: ShellDesktop[];
  activeDesktopId: string;
};

type PersistedV1 = {
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
  if (tab.kind === "lme") {
    // Prefer the open tab’s family — explorerMode is intentionally not synced on activate.
    // Inline map (avoid importing lmeExplorerModes → circular init with lmeWorkspace).
    const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
    switch (lme?.kind) {
      case "script":
      case "manuscript":
      case "flow":
      case "schedule":
        return "automations";
      case "note":
      case "file":
      case "deck":
        return "library";
      default:
        return "library";
    }
  }
  if (tab.kind === "web") return "web";
  return tab.surfaceId;
}

function newDesktopId(): string {
  return `desktop-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

function emptyLayout(): ShellDesktopLayout {
  return {
    tabs: [],
    groups: [{ id: MAIN_GROUP_ID, tabIds: [], activeTabId: null }],
    splitRoot: { type: "group", id: MAIN_GROUP_ID },
    activeGroupId: MAIN_GROUP_ID,
    zoomedGroupId: null,
  };
}

function isValidLayout(parsed: Partial<ShellDesktopLayout> | null | undefined): parsed is ShellDesktopLayout {
  return Boolean(
    parsed?.tabs &&
      parsed?.groups?.length &&
      parsed?.splitRoot &&
      parsed?.activeGroupId,
  );
}

function layoutFromV1(v1: PersistedV1): ShellDesktopLayout | null {
  if (!v1?.tabs || !v1?.group) return null;
  const group = v1.group.id ? v1.group : { ...v1.group, id: MAIN_GROUP_ID };
  return {
    tabs: v1.tabs,
    groups: [group],
    splitRoot: migrateV1ToSplitRoot(group.id),
    activeGroupId: group.id,
    zoomedGroupId: null,
  };
}

function loadPersisted(): PersistedV3 | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const rawV3 = localStorage.getItem(PERSIST_KEY);
    if (rawV3) {
      const parsed = JSON.parse(rawV3) as PersistedV3;
      if (
        Array.isArray(parsed?.desktops) &&
        parsed.desktops.length > 0 &&
        parsed.activeDesktopId &&
        parsed.desktops.every(
          (desktop) =>
            desktop?.id &&
            desktop?.name &&
            isValidLayout(desktop.layout),
        )
      ) {
        return parsed;
      }
    }

    const rawV2 = localStorage.getItem(PERSIST_KEY_V2);
    if (rawV2) {
      const layout = JSON.parse(rawV2) as PersistedV2;
      if (isValidLayout(layout)) {
        const id = newDesktopId();
        return {
          desktops: [{ id, name: DEFAULT_DESKTOP_NAME, layout }],
          activeDesktopId: id,
        };
      }
    }

    const rawV1 = localStorage.getItem(PERSIST_KEY_V1);
    if (!rawV1) return null;
    const v1 = JSON.parse(rawV1) as PersistedV1;
    const layout = layoutFromV1(v1);
    if (!layout) return null;
    const id = newDesktopId();
    return {
      desktops: [{ id, name: DEFAULT_DESKTOP_NAME, layout }],
      activeDesktopId: id,
    };
  } catch {
    return null;
  }
}

export class ShellTabsStore {
  tabs = $state<ShellTab[]>([]);
  groups = $state<EditorGroup[]>([
    { id: MAIN_GROUP_ID, tabIds: [], activeTabId: null },
  ]);
  splitRoot = $state<SplitNode>({ type: "group", id: MAIN_GROUP_ID });
  activeGroupId = $state(MAIN_GROUP_ID);
  zoomedGroupId = $state<string | null>(null);
  desktops = $state<ShellDesktop[]>([]);
  activeDesktopId = $state<string>("");
  /** Pane under an in-progress shell-tab drag (highlight). */
  tabDropTargetGroupId = $state<string | null>(null);
  /** Spotlight / commands request the pane cheat sheet. */
  cheatSheetOpenRequest = $state(0);
  /** Force-show tabs in a pane until timestamp (Ctrl+; w). */
  forceShowTabsUntil = $state(0);
  forceShowTabsGroupId = $state<string | null>(null);

  /** Cursor-style tab visit history for rail back/forward. */
  navBackStack = $state<string[]>([]);
  navForwardStack = $state<string[]>([]);

  private bootstrapped = false;
  private suppressMirrorDepth = 0;
  private navQuiet = false;

  private get suppressMirror() {
    return this.suppressMirrorDepth > 0;
  }

  private beginSuppressMirror() {
    this.suppressMirrorDepth += 1;
  }

  private endSuppressMirror() {
    this.suppressMirrorDepth = Math.max(0, this.suppressMirrorDepth - 1);
  }

  activeGroup = $derived(
    this.groups.find((group) => group.id === this.activeGroupId) ?? this.groups[0]!,
  );

  mainGroup = $derived(this.activeGroup);

  activeTabId = $derived(this.activeGroup.activeTabId);

  activeTab = $derived.by(() => {
    const id = this.activeTabId;
    if (!id) return null;
    return this.tabs.find((tab) => tab.id === id) ?? null;
  });

  canGoNavBack = $derived(this.navBackStack.length > 0);
  canGoNavForward = $derived(this.navForwardStack.length > 0);

  orderedTabs = $derived.by(() => this.tabsForGroup(this.activeGroupId));

  paneCount = $derived(countLeaves(this.splitRoot));

  activeDesktop = $derived(
    this.desktops.find((desktop) => desktop.id === this.activeDesktopId) ??
      this.desktops[0] ??
      null,
  );

  activeDesktopName = $derived(this.activeDesktop?.name ?? DEFAULT_DESKTOP_NAME);

  private captureLayout(): ShellDesktopLayout {
    return {
      tabs: this.tabs,
      groups: this.groups,
      splitRoot: this.splitRoot,
      activeGroupId: this.activeGroupId,
      zoomedGroupId: this.zoomedGroupId,
    };
  }

  private applyLayout(layout: ShellDesktopLayout) {
    this.tabs = layout.tabs;
    this.groups = layout.groups.length
      ? layout.groups
      : [{ id: MAIN_GROUP_ID, tabIds: [], activeTabId: null }];
    this.splitRoot = layout.splitRoot;
    this.activeGroupId = layout.activeGroupId || this.groups[0]!.id;
    this.zoomedGroupId = layout.zoomedGroupId ?? null;
  }

  /**
   * Write the live layout into the active desktop slot.
   * Only call when switching / renaming / removing — not on every persist.
   * Reassigning `desktops` from ShellTabHost `$effect` sync paths would
   * re-trigger those effects and freeze the UI main thread.
   */
  private flushActiveDesktop() {
    if (!this.activeDesktopId || this.desktops.length === 0) return;
    const layout = this.captureLayout();
    this.desktops = this.desktops.map((desktop) =>
      desktop.id === this.activeDesktopId ? { ...desktop, layout } : desktop,
    );
  }

  private ensureDesktopCatalog() {
    if (this.desktops.length > 0 && this.activeDesktopId) return;
    const id = newDesktopId();
    this.desktops = [{ id, name: DEFAULT_DESKTOP_NAME, layout: this.captureLayout() }];
    this.activeDesktopId = id;
  }

  /** Persist v3 without mutating reactive `desktops` (active layout is live state). */
  private persist() {
    if (typeof localStorage === "undefined") return;
    try {
      this.ensureDesktopCatalog();
      const layout = this.captureLayout();
      const desktops = this.desktops.map((desktop) =>
        desktop.id === this.activeDesktopId ? { ...desktop, layout } : desktop,
      );
      const payload: PersistedV3 = {
        desktops,
        activeDesktopId: this.activeDesktopId,
      };
      localStorage.setItem(PERSIST_KEY, JSON.stringify(payload));
    } catch {
      /* ignore */
    }
  }

  private async resyncLiveStreams(previousIds: string[]) {
    const nextIds = this.chatSessionIdsForLiveRestore();
    const nextSet = new Set(nextIds);
    for (const sessionId of previousIds) {
      if (!nextSet.has(sessionId)) {
        chatStreamPool.release(sessionId);
      }
    }
    for (const sessionId of nextIds) {
      chatStreamPool.acquire(sessionId);
    }
    const principal = chat.sessionId?.trim() ?? "";
    for (const sessionId of nextIds) {
      if (sessionId !== principal) {
        void chat.warmBackgroundSession(sessionId);
      }
    }
  }

  tabsForGroup(groupId: string): ShellTab[] {
    const group = this.groups.find((entry) => entry.id === groupId);
    if (!group) return [];
    const byId = new Map(this.tabs.map((tab) => [tab.id, tab]));
    return group.tabIds
      .map((id) => byId.get(id))
      .filter((tab): tab is ShellTab => Boolean(tab));
  }

  groupForTab(tabId: string): EditorGroup | null {
    return this.groups.find((group) => group.tabIds.includes(tabId)) ?? null;
  }

  private syncLayoutHint(tab: ShellTab | null) {
    const surface = focusSurfaceHint(tab);
    if (!surface) return;
    layout.focusDesktopSurface(surface);
  }

  private patchGroup(groupId: string, patch: Partial<EditorGroup>) {
    this.groups = this.groups.map((group) =>
      group.id === groupId ? { ...group, ...patch } : group,
    );
  }

  private removeTabFromAllGroups(tabId: string) {
    this.groups = this.groups.map((group) => {
      if (!group.tabIds.includes(tabId)) return group;
      const tabIds = group.tabIds.filter((id) => id !== tabId);
      let activeTabId = group.activeTabId;
      if (activeTabId === tabId) {
        activeTabId = tabIds[tabIds.length - 1] ?? null;
      }
      return { ...group, tabIds, activeTabId };
    });
    this.tabs = this.tabs.filter((tab) => tab.id !== tabId);
  }

  private enforceCap(preferKeepId?: string) {
    while (this.tabs.length > MAX_TABS) {
      const drop =
        this.tabs.find((tab) => tab.id !== preferKeepId && tab.id !== this.activeTabId)?.id ??
        this.tabs[0]?.id;
      if (!drop) break;
      this.removeTabFromAllGroups(drop);
    }
  }

  private insertTabIntoGroup(tab: ShellTab, groupId: string, activate: boolean) {
    this.tabs = [...this.tabs, tab];
    const group = this.groups.find((entry) => entry.id === groupId);
    if (!group) return;
    const tabIds = [...group.tabIds, tab.id];
    const activeTabId = activate ? tab.id : group.activeTabId;
    this.patchGroup(groupId, { tabIds, activeTabId });
    if (activate) {
      this.activeGroupId = groupId;
      this.syncLayoutHint(tab);
    }
    this.enforceCap(tab.id);
    this.persist();
  }

  private findChatTabInGroup(sessionId: string, groupId: string): ShellTab | undefined {
    const group = this.groups.find((entry) => entry.id === groupId);
    if (!group) return undefined;
    return this.tabs.find(
      (tab) =>
        tab.kind === "chat" &&
        tab.sessionId === sessionId &&
        group.tabIds.includes(tab.id),
    );
  }

  /**
   * Unique chat session ids to re-acquire as live on restart.
   * Active pane first, then remaining leaves in visual order.
   */
  chatSessionIdsForLiveRestore(): string[] {
    const ids: string[] = [];
    const seen = new Set<string>();
    const pushActiveChat = (groupId: string) => {
      const group = this.groups.find((entry) => entry.id === groupId);
      if (!group?.activeTabId) return;
      const tab = this.tabs.find((entry) => entry.id === group.activeTabId);
      if (tab?.kind !== "chat") return;
      const sessionId = tab.sessionId.trim();
      if (!sessionId || seen.has(sessionId)) return;
      seen.add(sessionId);
      ids.push(sessionId);
    };
    pushActiveChat(this.activeGroupId);
    for (const groupId of leafOrder(this.splitRoot)) {
      pushActiveChat(groupId);
    }
    return ids;
  }

  bootstrap() {
    if (this.bootstrapped) return;
    this.bootstrapped = true;

    const persisted = loadPersisted();
    if (persisted) {
      this.desktops = persisted.desktops;
      this.activeDesktopId = persisted.activeDesktopId;
      const activeDesktop =
        persisted.desktops.find((desktop) => desktop.id === persisted.activeDesktopId) ??
        persisted.desktops[0]!;
      this.applyLayout(activeDesktop.layout);
      if (this.tabs.length > 0) {
        const active = this.activeTab;
        if (active) {
          void this.activate(active.id, { skipOpen: true });
        }
        this.persist();
        return;
      }
    } else {
      this.ensureDesktopCatalog();
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
    options?: { activate?: boolean; title?: string; groupId?: string },
  ): string | null {
    const trimmed = sessionId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const groupId = options?.groupId ?? this.activeGroupId;

    const existingInGroup = this.findChatTabInGroup(trimmed, groupId);
    if (existingInGroup) {
      if (options?.title) this.patchTitle(existingInGroup.id, options.title);
      if (activate) void this.activate(existingInGroup.id);
      return existingInGroup.id;
    }

    // Same session already open in another pane — focus it (unless split passed groupId).
    if (activate && options?.groupId === undefined) {
      const elsewhere = this.tabs.find(
        (tab) => tab.kind === "chat" && tab.sessionId === trimmed,
      );
      if (elsewhere) {
        if (options?.title) this.patchTitle(elsewhere.id, options.title);
        void this.activate(elsewhere.id);
        return elsewhere.id;
      }
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
    this.insertTabIntoGroup(tab, groupId, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openLme(
    lmeTabId: string,
    options?: { activate?: boolean; title?: string; groupId?: string },
  ): string | null {
    const trimmed = lmeTabId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const groupId = options?.groupId ?? this.activeGroupId;
    const lmeTab = lmeWorkspace.tabs.find((tab) => tab.tabId === trimmed);
    const title =
      options?.title?.trim() || lmeTab?.title?.trim() || "Document";

    const existingInGroup = this.tabs.find(
      (tab) =>
        tab.kind === "lme" &&
        tab.lmeTabId === trimmed &&
        this.groupForTab(tab.id)?.id === groupId,
    );
    if (existingInGroup) {
      this.patchTitle(existingInGroup.id, title);
      if (activate) void this.activate(existingInGroup.id);
      return existingInGroup.id;
    }

    // Same document elsewhere — focus it unless split passed an explicit groupId.
    if (activate && options?.groupId === undefined) {
      const elsewhere = this.tabs.find(
        (tab) => tab.kind === "lme" && tab.lmeTabId === trimmed,
      );
      if (elsewhere) {
        this.patchTitle(elsewhere.id, title);
        void this.activate(elsewhere.id);
        return elsewhere.id;
      }
    }

    const librarySurface = this.tabs.find(
      (tab) => tab.kind === "surface" && tab.surfaceId === "library",
    );
    if (librarySurface) {
      this.removeTabFromAllGroups(librarySurface.id);
    }
    const tab: ShellTab = {
      id: newTabId("lme"),
      kind: "lme",
      lmeTabId: trimmed,
      title,
    };
    this.insertTabIntoGroup(tab, groupId, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openWeb(
    browserTabId: string,
    options?: { activate?: boolean; title?: string; groupId?: string },
  ): string | null {
    const trimmed = browserTabId.trim();
    if (!trimmed) return null;
    const activate = options?.activate !== false;
    const groupId = options?.groupId ?? this.activeGroupId;
    const browserTab = humanBrowser.tabs.find((tab) => tab.id === trimmed);
    const title =
      options?.title?.trim() ||
      (browserTab ? tabDisplayLabel(browserTab.title, browserTab.url) : "Web");

    const existingInGroup = this.tabs.find(
      (tab) =>
        tab.kind === "web" &&
        tab.browserTabId === trimmed &&
        this.groupForTab(tab.id)?.id === groupId,
    );
    if (existingInGroup) {
      this.patchTitle(existingInGroup.id, title);
      if (activate) void this.activate(existingInGroup.id);
      return existingInGroup.id;
    }

    if (activate && options?.groupId === undefined) {
      const elsewhere = this.tabs.find(
        (tab) => tab.kind === "web" && tab.browserTabId === trimmed,
      );
      if (elsewhere) {
        this.patchTitle(elsewhere.id, title);
        void this.activate(elsewhere.id);
        return elsewhere.id;
      }
    }

    const tab: ShellTab = {
      id: newTabId("web"),
      kind: "web",
      browserTabId: trimmed,
      title,
    };
    this.insertTabIntoGroup(tab, groupId, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openSurface(
    surfaceId: string,
    options?: { activate?: boolean; groupId?: string },
  ): string | null {
    let next = surfaceId === "home" ? "chat" : surfaceId;
    if (next === "automations" || next === "workshop") next = "library";
    const groupId = options?.groupId ?? this.activeGroupId;
    if (next === "chat") {
      const sessionId = chat.sessionId?.trim();
      if (sessionId) {
        return this.openChat(sessionId, {
          activate: options?.activate !== false,
          groupId,
        });
      }
    }
    if (next === "web") {
      const browserTab = humanBrowser.activeTab;
      if (browserTab) {
        return this.openWeb(browserTab.id, {
          activate: options?.activate !== false,
          groupId,
        });
      }
      void humanBrowser.openTab("about:blank").then(() => {
        const created = humanBrowser.activeTab;
        if (created) this.openWeb(created.id, { activate: true, groupId });
      });
      return null;
    }
    const activate = options?.activate !== false;
    const existingInGroup = this.tabs.find(
      (tab) =>
        tab.kind === "surface" &&
        tab.surfaceId === next &&
        this.groupForTab(tab.id)?.id === groupId,
    );
    if (existingInGroup) {
      if (activate) void this.activate(existingInGroup.id);
      return existingInGroup.id;
    }

    // Singleton surfaces focus elsewhere unless split passed an explicit groupId.
    if (activate && options?.groupId === undefined) {
      const elsewhere = this.tabs.find(
        (tab) => tab.kind === "surface" && tab.surfaceId === next,
      );
      if (elsewhere) {
        void this.activate(elsewhere.id);
        return elsewhere.id;
      }
    }

    const tab: ShellTab = {
      id: newTabId("surface"),
      kind: "surface",
      surfaceId: next as Surface,
      title: surfaceTitle(next),
    };
    this.insertTabIntoGroup(tab, groupId, false);
    if (activate) void this.activate(tab.id);
    else this.persist();
    return tab.id;
  }

  openDestination(surfaceId: string) {
    this.openSurface(surfaceId, { activate: true });
  }

  private recordNavVisit(nextTabId: string) {
    if (this.navQuiet) return;
    const current = this.activeTabId;
    if (!current || current === nextTabId) return;
    if (!this.tabs.some((tab) => tab.id === current)) return;
    this.navBackStack = [...this.navBackStack, current].slice(-40);
    this.navForwardStack = [];
  }

  private pruneNavStacks() {
    const alive = new Set(this.tabs.map((tab) => tab.id));
    this.navBackStack = this.navBackStack.filter((id) => alive.has(id));
    this.navForwardStack = this.navForwardStack.filter((id) => alive.has(id));
  }

  async goNavBack() {
    while (this.navBackStack.length > 0) {
      const prev = this.navBackStack[this.navBackStack.length - 1]!;
      this.navBackStack = this.navBackStack.slice(0, -1);
      if (!this.tabs.some((tab) => tab.id === prev)) continue;
      const current = this.activeTabId;
      if (current) this.navForwardStack = [...this.navForwardStack, current];
      this.navQuiet = true;
      try {
        await this.activate(prev);
      } finally {
        this.navQuiet = false;
      }
      return;
    }
  }

  async goNavForward() {
    while (this.navForwardStack.length > 0) {
      const next = this.navForwardStack[this.navForwardStack.length - 1]!;
      this.navForwardStack = this.navForwardStack.slice(0, -1);
      if (!this.tabs.some((tab) => tab.id === next)) continue;
      const current = this.activeTabId;
      if (current) this.navBackStack = [...this.navBackStack, current];
      this.navQuiet = true;
      try {
        await this.activate(next);
      } finally {
        this.navQuiet = false;
      }
      return;
    }
  }

  async activate(tabId: string, options?: { skipOpen?: boolean }) {
    const tab = this.tabs.find((entry) => entry.id === tabId);
    if (!tab) return;

    const previous = this.activeTab;
    const leavingLmeNote =
      previous &&
      previous.id !== tabId &&
      previous.kind === "lme" &&
      lmeWorkspace.activeTab?.kind === "note";
    if (leavingLmeNote) {
      // Flush vault drafts before shell remounts / swaps the active host.
      const ok = await vault.flushBeforeLeave();
      if (!ok) return;
    }

    if (tabId !== this.activeTabId) {
      this.recordNavVisit(tabId);
    }

    const host = this.groupForTab(tabId);
    if (host) {
      this.activeGroupId = host.id;
      this.patchGroup(host.id, { activeTabId: tabId });
    }
    this.syncLayoutHint(tab);
    this.persist();

    this.beginSuppressMirror();
    try {
      if (tab.kind === "chat") {
        chatStreamPool.acquire(tab.sessionId);
        if (chat.sessionId !== tab.sessionId) {
          await chat.switchSession(tab.sessionId);
        }
        return;
      }
      if (tab.kind === "lme") {
        if (lmeWorkspace.activeTabId !== tab.lmeTabId) {
          await lmeWorkspace.activateTab(tab.lmeTabId);
        } else {
          // Same LME tab (e.g. pane focus) — still promote vault focus if it drifted.
          const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
          if (lme?.kind === "note" && !vault.isFocusedPath(lme.path)) {
            await vault.openNote(lme.path);
          }
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
    void options;
  }

  mirrorLmeTab(lmeTabId: string, options?: { activate?: boolean; title?: string }) {
    if (this.suppressMirror) return;
    this.openLme(lmeTabId, {
      activate: options?.activate !== false,
      title: options?.title,
    });
  }

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
    const host = this.groupForTab(tabId);
    const wasActive = this.activeTabId === tabId && host?.id === this.activeGroupId;
    this.removeTabFromAllGroups(tabId);
    this.pruneNavStacks();

    this.beginSuppressMirror();
    try {
      if (tab.kind === "lme") {
        const stillOpen = this.tabs.some(
          (entry) => entry.kind === "lme" && entry.lmeTabId === tab.lmeTabId,
        );
        if (!stillOpen) {
          void lmeWorkspace.closeTab(tab.lmeTabId, { activateNext: false });
        }
      } else if (tab.kind === "web") {
        const stillOpen = this.tabs.some(
          (entry) => entry.kind === "web" && entry.browserTabId === tab.browserTabId,
        );
        if (!stillOpen) {
          void humanBrowser.closeTab(tab.browserTabId);
        }
      } else if (tab.kind === "chat") {
        const stillOpen = this.tabs.some(
          (entry) => entry.kind === "chat" && entry.sessionId === tab.sessionId,
        );
        if (!stillOpen) {
          chatStreamPool.release(tab.sessionId);
        }
      }
    } finally {
      this.endSuppressMirror();
    }

    const group = host
      ? this.groups.find((entry) => entry.id === host.id)
      : this.activeGroup;
    if (wasActive && group?.activeTabId) {
      void this.activate(group.activeTabId);
    }
    // Empty group stays empty — ShellPane shows “Open something from the rail.”
    // Do not auto-open library/chat placeholders (felt like stuck empty workspace tabs).
    this.persist();
  }

  moveTab(tabId: string, toGroupId: string) {
    const tab = this.tabs.find((entry) => entry.id === tabId);
    const to = this.groups.find((group) => group.id === toGroupId);
    if (!tab || !to) return;
    const from = this.groupForTab(tabId);
    if (!from || from.id === toGroupId) return;

    const fromTabs = from.tabIds.filter((id) => id !== tabId);
    let fromActive = from.activeTabId;
    if (fromActive === tabId) {
      fromActive = fromTabs[fromTabs.length - 1] ?? null;
    }
    this.patchGroup(from.id, { tabIds: fromTabs, activeTabId: fromActive });
    this.patchGroup(toGroupId, {
      tabIds: [...to.tabIds, tabId],
      activeTabId: tabId,
    });
    this.activeGroupId = toGroupId;
    this.syncLayoutHint(tab);
    this.persist();
  }

  requestCheatSheet() {
    this.cheatSheetOpenRequest += 1;
  }

  splitActive(direction: SplitDirection): boolean {
    if (countLeaves(this.splitRoot) >= MAX_SHELL_PANES) {
      return false;
    }
    const fromGroupId = this.activeGroupId;
    const seed = this.activeTab;
    const newGroupId = newSplitId("group");
    const result = splitLeaf(this.splitRoot, fromGroupId, direction, newGroupId);
    if (!result) return false;

    this.splitRoot = result.root;
    this.groups = [...this.groups, { id: newGroupId, tabIds: [], activeTabId: null }];

    if (seed) {
      // Move the focused tab into the new pane — do not clone. Shared LME/web
      // identities meant closing one shell tab tore down both views.
      this.moveTab(seed.id, newGroupId);
      void this.activate(seed.id);
    } else {
      this.activeGroupId = newGroupId;
      this.openSurface("library", { activate: true, groupId: newGroupId });
    }
    this.persist();
    return true;
  }

  focusGroup(groupId: string) {
    if (!this.groups.some((group) => group.id === groupId)) return;
    this.activeGroupId = groupId;
    const group = this.groups.find((entry) => entry.id === groupId);
    if (group?.activeTabId) {
      void this.activate(group.activeTabId);
    } else {
      this.syncLayoutHint(null);
      this.persist();
    }
  }

  focusDirection(dir: FocusDir) {
    const next = neighborInDirection(this.splitRoot, this.activeGroupId, dir);
    if (next) this.focusGroup(next);
  }

  focusPaneIndex(index: number) {
    const order = leafOrder(this.splitRoot);
    const id = order[index];
    if (id) this.focusGroup(id);
  }

  closeActiveGroup(): boolean {
    if (countLeaves(this.splitRoot) <= 1) return false;
    const closingId = this.activeGroupId;
    const result = removeLeaf(this.splitRoot, closingId);
    if (!result.removed) return false;

    const closing = this.groups.find((group) => group.id === closingId);
    const tabIds = closing?.tabIds ?? [];
    for (const tabId of tabIds) {
      const tab = this.tabs.find((entry) => entry.id === tabId);
      if (!tab) continue;
      this.beginSuppressMirror();
      try {
        if (tab.kind === "lme") {
          const stillOpen = this.tabs.some(
            (entry) =>
              entry.id !== tabId &&
              entry.kind === "lme" &&
              entry.lmeTabId === tab.lmeTabId,
          );
          if (!stillOpen) {
            void lmeWorkspace.closeTab(tab.lmeTabId, { activateNext: false });
          }
        } else if (tab.kind === "web") {
          const stillOpen = this.tabs.some(
            (entry) =>
              entry.id !== tabId &&
              entry.kind === "web" &&
              entry.browserTabId === tab.browserTabId,
          );
          if (!stillOpen) {
            void humanBrowser.closeTab(tab.browserTabId);
          }
        } else if (tab.kind === "chat") {
          const stillOpen = this.tabs.some(
            (entry) =>
              entry.id !== tabId &&
              entry.kind === "chat" &&
              entry.sessionId === tab.sessionId,
          );
          if (!stillOpen) {
            chatStreamPool.release(tab.sessionId);
          }
        }
      } finally {
        this.endSuppressMirror();
      }
      this.tabs = this.tabs.filter((entry) => entry.id !== tabId);
    }

    this.groups = this.groups.filter((group) => group.id !== closingId);
    this.splitRoot = result.root;
    if (this.zoomedGroupId === closingId) {
      this.zoomedGroupId = null;
    }
    const remaining = collectGroupIds(this.splitRoot);
    this.activeGroupId = remaining[remaining.length - 1] ?? MAIN_GROUP_ID;
    const active = this.activeGroup;
    if (active.activeTabId) {
      void this.activate(active.activeTabId);
    }
    this.persist();
    return true;
  }

  setRatio(branchId: string, ratio: number) {
    this.splitRoot = setBranchRatio(this.splitRoot, branchId, clampRatio(ratio));
    this.persist();
  }

  zoomToggle() {
    if (this.zoomedGroupId) {
      this.zoomedGroupId = null;
    } else {
      this.zoomedGroupId = this.activeGroupId;
    }
    this.persist();
  }

  clearZoom() {
    if (!this.zoomedGroupId) return;
    this.zoomedGroupId = null;
    this.persist();
  }

  nextTabInActiveGroup() {
    const tabs = this.tabsForGroup(this.activeGroupId);
    if (tabs.length < 2) return;
    const idx = tabs.findIndex((tab) => tab.id === this.activeTabId);
    const next = tabs[(idx + 1) % tabs.length];
    if (next) void this.activate(next.id);
  }

  prevTabInActiveGroup() {
    const tabs = this.tabsForGroup(this.activeGroupId);
    if (tabs.length < 2) return;
    const idx = tabs.findIndex((tab) => tab.id === this.activeTabId);
    const next = tabs[(idx - 1 + tabs.length) % tabs.length];
    if (next) void this.activate(next.id);
  }

  flashTabs(groupId?: string) {
    this.forceShowTabsGroupId = groupId ?? this.activeGroupId;
    this.forceShowTabsUntil = Date.now() + 2000;
  }

  shouldForceShowTabs(groupId: string): boolean {
    return (
      this.forceShowTabsGroupId === groupId && Date.now() < this.forceShowTabsUntil
    );
  }

  patchTitle(tabId: string, title: string) {
    const trimmed = title.trim();
    if (!trimmed) return;
    this.tabs = this.tabs.map((tab) =>
      tab.id === tabId ? { ...tab, title: trimmed } : tab,
    );
    this.persist();
  }

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
        this.removeTabFromAllGroups(tab.id);
      }
    }
    this.persist();
  }

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
        this.removeTabFromAllGroups(tab.id);
      }
    }
    this.persist();
  }

  shouldKeepAlive(tab: ShellTab): boolean {
    if (tab.kind === "chat" || tab.kind === "web" || tab.kind === "lme") return true;
    return false;
  }

  /** Active chat session id for a pane (for stream pool / cache views). */
  chatSessionForGroup(groupId: string): string | null {
    const group = this.groups.find((entry) => entry.id === groupId);
    if (!group?.activeTabId) return null;
    const tab = this.tabs.find((entry) => entry.id === group.activeTabId);
    return tab?.kind === "chat" ? tab.sessionId : null;
  }

  createDesktop(name?: string): string {
    this.ensureDesktopCatalog();
    this.flushActiveDesktop();
    const trimmed = name?.trim() || `Desktop ${this.desktops.length + 1}`;
    const id = newDesktopId();
    this.desktops = [
      ...this.desktops,
      { id, name: trimmed, layout: emptyLayout() },
    ];
    this.persist();
    void this.switchDesktop(id);
    return id;
  }

  async switchDesktop(desktopId: string): Promise<boolean> {
    this.ensureDesktopCatalog();
    const trimmed = desktopId.trim();
    if (!trimmed || trimmed === this.activeDesktopId) return false;
    const target = this.desktops.find((desktop) => desktop.id === trimmed);
    if (!target) return false;

    const previousIds = this.chatSessionIdsForLiveRestore();
    this.flushActiveDesktop();
    this.applyLayout(target.layout);
    this.activeDesktopId = trimmed;
    this.persist();

    const active = this.activeTab;
    if (active) {
      await this.activate(active.id, { skipOpen: true });
    } else {
      this.syncLayoutHint(null);
    }
    await this.resyncLiveStreams(previousIds);
    return true;
  }

  renameDesktop(desktopId: string, name: string): boolean {
    const trimmedName = name.trim();
    if (!trimmedName) return false;
    const trimmedId = desktopId.trim() || this.activeDesktopId;
    if (!this.desktops.some((desktop) => desktop.id === trimmedId)) return false;
    this.flushActiveDesktop();
    this.desktops = this.desktops.map((desktop) =>
      desktop.id === trimmedId ? { ...desktop, name: trimmedName } : desktop,
    );
    this.persist();
    return true;
  }

  async removeDesktop(desktopId?: string): Promise<boolean> {
    this.ensureDesktopCatalog();
    if (this.desktops.length <= 1) return false;
    const trimmed = (desktopId ?? this.activeDesktopId).trim();
    const index = this.desktops.findIndex((desktop) => desktop.id === trimmed);
    if (index < 0) return false;

    const removingActive = trimmed === this.activeDesktopId;
    const previousIds = removingActive ? this.chatSessionIdsForLiveRestore() : [];
    this.flushActiveDesktop();
    const nextDesktops = this.desktops.filter((desktop) => desktop.id !== trimmed);
    const fallback =
      nextDesktops[Math.max(0, index - 1)] ?? nextDesktops[0]!;
    this.desktops = nextDesktops;

    if (removingActive) {
      this.applyLayout(fallback.layout);
      this.activeDesktopId = fallback.id;
      this.persist();
      const active = this.activeTab;
      if (active) {
        await this.activate(active.id, { skipOpen: true });
      } else {
        this.syncLayoutHint(null);
      }
      await this.resyncLiveStreams(previousIds);
    } else {
      this.persist();
    }
    return true;
  }

  cycleDesktop(delta = 1): void {
    this.ensureDesktopCatalog();
    if (this.desktops.length < 2) return;
    const index = this.desktops.findIndex(
      (desktop) => desktop.id === this.activeDesktopId,
    );
    const from = index < 0 ? 0 : index;
    const next =
      this.desktops[(from + delta + this.desktops.length) % this.desktops.length];
    if (next) void this.switchDesktop(next.id);
  }
}

export const shellTabs = new ShellTabsStore();
