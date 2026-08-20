/**
 * Shell-level tab host + binary split tree (TMUX-style panes).
 */

import { chatStreamPool } from "$lib/chat/chatStreamPool.svelte";
import { layout } from "$lib/runtime/layout.svelte";
import { shellTabFeaturePorts } from "$lib/runtime/shellTabPorts";
import type { LmeTab, LmeWorkspaceSession } from "$lib/stores/lmeWorkspace.svelte";
import { disposeDestinationFeatures } from "$lib/runtime/features/disposeDestinations";
import {
  isShellSurfaceTabId,
  MAX_SHELL_DESKTOPS,
  MAX_SHELL_PANES,
  type EditorGroup,
  type ShellDesktop,
  type ShellDesktopLayout,
  type ShellTab,
  type SplitDirection,
  type SplitEdge,
  type SplitNode,
} from "$lib/types/shellTabs";
import type { Surface } from "$lib/types/ui";
import { tabDisplayLabel } from "$lib/utils/browserFavicon";
import {
  chatPresenceOrSessionLabel,
  formatSessionLabel,
  presenceRoomTitle,
} from "$lib/utils/formatSession";
import { isChatLaneMessage } from "$lib/utils/askThreads";
import {
  clampRatio,
  collectGroupIds,
  countLeaves,
  leafOrder,
  mergeTargetForLeaf,
  migrateV1ToSplitRoot,
  neighborInDirection,
  newSplitId,
  removeLeaf,
  setBranchRatio,
  splitLeaf,
  splitLeafAtEdge,
  type FocusDir,
} from "$lib/utils/shellSplitTree";
import {
  titleOfTab,
  type ShellTabSearchHit,
} from "$lib/utils/shellTabSearch";
import type { HomeOnboardingLayout } from "$lib/utils/homeOnboarding";

const MAX_TABS = 16;
const MAIN_GROUP_ID = "main";
const DEFAULT_DESKTOP_NAME = "Main";
const PERSIST_KEY_V1 = "medousa-home-shell-tabs-v1";
const PERSIST_KEY_V2 = "medousa-home-shell-tabs-v2";
const PERSIST_KEY_V3 = "medousa-home-shell-tabs-v3";
const PERSIST_KEY = "medousa-home-workspace-session-v4";
const PERSONAL_WORKSPACE_SCOPE = "personal";

function ports() {
  return shellTabFeaturePorts();
}

function persistenceKey(scopeId: string): string {
  return `${PERSIST_KEY}:${encodeURIComponent(scopeId)}`;
}

type PersistedV2 = ShellDesktopLayout;

type PersistedV3 = {
  desktops: ShellDesktop[];
  activeDesktopId: string;
};

type PersistedV4 = PersistedV3 & {
  version: 4;
  savedAt: number;
  lme: LmeWorkspaceSession;
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
    case "code":
      return "Code";
    case "chat":
      return "Chat";
    case "peers":
      return "Peers";
    case "messaging":
      return "Channels";
    case "context":
    case "map":
      return "Map";
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
    const lme = ports().lme.tabs().find((entry) => entry.tabId === tab.lmeTabId);
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
      case "code":
        return "code";
      default:
        return "library";
    }
  }
  if (tab.kind === "web") return "web";
  if (tab.kind === "terminal") return null;
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

/** Append shell tabs onto a desktop layout's focused (or first) pane. */
function appendTabsToLayout(
  layout: ShellDesktopLayout,
  incoming: ShellTab[],
): ShellDesktopLayout {
  if (incoming.length === 0) return layout;
  const destGroupId =
    layout.groups.some((group) => group.id === layout.activeGroupId)
      ? layout.activeGroupId
      : layout.groups[0]?.id ?? MAIN_GROUP_ID;
  const existingIds = new Set(layout.tabs.map((tab) => tab.id));
  const fresh = incoming.filter((tab) => !existingIds.has(tab.id));
  if (fresh.length === 0) return layout;
  const freshIds = fresh.map((tab) => tab.id);
  return {
    ...layout,
    tabs: [...layout.tabs, ...fresh],
    groups: layout.groups.map((group) =>
      group.id === destGroupId
        ? {
            ...group,
            tabIds: [...group.tabIds, ...freshIds],
            activeTabId: freshIds[freshIds.length - 1] ?? group.activeTabId,
          }
        : group,
    ),
    activeGroupId: destGroupId,
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

function migrateCodeLmeSession(desktops: ShellDesktop[]): LmeWorkspaceSession {
  const tabs = new Map<string, LmeTab>();
  let activeTabId: string | null = null;
  for (const desktop of desktops) {
    const activeShellId = desktop.layout.groups.find(
      (group) => group.id === desktop.layout.activeGroupId,
    )?.activeTabId;
    for (const shellTab of desktop.layout.tabs) {
      if (shellTab.kind !== "lme") continue;
      const parts = shellTab.lmeTabId.split(":");
      let tab: LmeTab | null = null;
      try {
        if (parts[0] === "code-file" && parts.length === 3) {
          const workId = decodeURIComponent(parts[1]!);
          const path = decodeURIComponent(parts[2]!);
          tab = {
            tabId: shellTab.lmeTabId,
            kind: "code",
            workId,
            title: shellTab.title,
            resource: { kind: "file", path, line: null },
          };
        } else if ((parts[0] === "code-workspace" || parts[0] === "code-review") && parts.length === 2) {
          tab = {
            tabId: shellTab.lmeTabId,
            kind: "code",
            workId: decodeURIComponent(parts[1]!),
            title: shellTab.title,
            resource: { kind: parts[0] === "code-review" ? "review" : "workspace" },
          };
        }
      } catch {
        tab = null;
      }
      if (!tab) continue;
      tabs.set(tab.tabId, tab);
      if (shellTab.id === activeShellId) activeTabId = tab.tabId;
    }
  }
  const restored = [...tabs.values()];
  return { tabs: restored, activeTabId: activeTabId ?? restored.at(-1)?.tabId ?? null };
}

function filterLayoutForLme(
  layout: ShellDesktopLayout,
  lmeTabIds: Set<string>,
): ShellDesktopLayout {
  const seenTabs = new Set<string>();
  const tabs = layout.tabs.filter((tab) => {
    if (seenTabs.has(tab.id) || (tab.kind === "lme" && !lmeTabIds.has(tab.lmeTabId))) {
      return false;
    }
    seenTabs.add(tab.id);
    return true;
  });
  const tabIds = new Set(tabs.map((tab) => tab.id));
  const groupById = new Map(layout.groups.map((group) => [group.id, group]));
  const leafIds = collectGroupIds(layout.splitRoot);
  const groups = leafIds.map((id) => {
    const group = groupById.get(id) ?? { id, tabIds: [], activeTabId: null };
    const nextIds = [...new Set(group.tabIds.filter((tabId) => tabIds.has(tabId)))];
    return {
      ...group,
      tabIds: nextIds,
      activeTabId: group.activeTabId && nextIds.includes(group.activeTabId)
        ? group.activeTabId
        : nextIds.at(-1) ?? null,
    };
  });
  const activeGroupId = leafIds.includes(layout.activeGroupId)
    ? layout.activeGroupId
    : leafIds[0] ?? MAIN_GROUP_ID;
  return {
    ...layout,
    tabs,
    groups,
    activeGroupId,
    zoomedGroupId: layout.zoomedGroupId && leafIds.includes(layout.zoomedGroupId)
      ? layout.zoomedGroupId
      : null,
  };
}

function loadPersisted(scopeId: string): PersistedV4 | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const rawV4 = localStorage.getItem(persistenceKey(scopeId));
    if (rawV4) {
      const parsed = JSON.parse(rawV4) as PersistedV4;
      if (
        parsed?.version === 4 &&
        Array.isArray(parsed?.desktops) &&
        parsed.desktops.length > 0 &&
        parsed.activeDesktopId &&
        parsed.lme &&
        parsed.desktops.every(
          (desktop) => desktop?.id && desktop?.name && isValidLayout(desktop.layout),
        )
      ) {
        return parsed;
      }
    }

    // The old keys predate workshops. They can only belong to Personal; never
    // import an unscoped snapshot into a remote workshop.
    if (scopeId !== PERSONAL_WORKSPACE_SCOPE) return null;

    const legacyV4 = localStorage.getItem(PERSIST_KEY);
    if (legacyV4) {
      const parsed = JSON.parse(legacyV4) as PersistedV4;
      if (
        parsed?.version === 4 &&
        Array.isArray(parsed?.desktops) &&
        parsed.desktops.length > 0 &&
        parsed.activeDesktopId &&
        parsed.lme &&
        parsed.desktops.every(
          (desktop) => desktop?.id && desktop?.name && isValidLayout(desktop.layout),
        )
      ) {
        return parsed;
      }
    }

    const rawV3 = localStorage.getItem(PERSIST_KEY_V3);
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
        return {
          ...parsed,
          version: 4,
          savedAt: Date.now(),
          lme: migrateCodeLmeSession(parsed.desktops),
        };
      }
    }

    const rawV2 = localStorage.getItem(PERSIST_KEY_V2);
    if (rawV2) {
      const layout = JSON.parse(rawV2) as PersistedV2;
      if (isValidLayout(layout)) {
        const id = newDesktopId();
        return {
          version: 4,
          savedAt: Date.now(),
          desktops: [{ id, name: DEFAULT_DESKTOP_NAME, layout }],
          activeDesktopId: id,
          lme: { tabs: [], activeTabId: null },
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
      version: 4,
      savedAt: Date.now(),
      desktops: [{ id, name: DEFAULT_DESKTOP_NAME, layout }],
      activeDesktopId: id,
      lme: { tabs: [], activeTabId: null },
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
  /** Edge highlight while dragging a tab to split. */
  tabDropSplitEdge = $state<{ groupId: string; edge: SplitEdge } | null>(null);
  /** Spotlight / commands request the pane cheat sheet. */
  cheatSheetOpenRequest = $state(0);
  /** Force-show tabs in a pane until timestamp (Ctrl+; w). */
  forceShowTabsUntil = $state(0);
  forceShowTabsGroupId = $state<string | null>(null);

  /** Cursor-style tab visit history for rail back/forward. */
  navBackStack = $state<string[]>([]);
  navForwardStack = $state<string[]>([]);

  private bootstrapped = false;
  private workspaceScopeId = PERSONAL_WORKSPACE_SCOPE;
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
  canCreateDesktop = $derived(this.desktops.length < MAX_SHELL_DESKTOPS);

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
    if (!this.bootstrapped) return;
    if (typeof localStorage === "undefined") return;
    try {
      this.ensureDesktopCatalog();
      const layout = this.captureLayout();
      const lme = ports().lme.captureSession();
      const lmeTabIds = new Set(lme.tabs.map((tab) => tab.tabId));
      const desktops = this.desktops.map((desktop) =>
        desktop.id === this.activeDesktopId ? { ...desktop, layout } : desktop,
      ).map((desktop) => ({
        ...desktop,
        layout: filterLayoutForLme(desktop.layout, lmeTabIds),
      }));
      const payload: PersistedV4 = {
        version: 4,
        savedAt: Date.now(),
        desktops,
        activeDesktopId: this.activeDesktopId,
        lme,
      };
      localStorage.setItem(persistenceKey(this.workspaceScopeId), JSON.stringify(payload));
      if (this.workspaceScopeId === PERSONAL_WORKSPACE_SCOPE) {
        localStorage.removeItem(PERSIST_KEY);
        localStorage.removeItem(PERSIST_KEY_V3);
        localStorage.removeItem(PERSIST_KEY_V2);
        localStorage.removeItem(PERSIST_KEY_V1);
      }
    } catch {
      /* ignore */
    }
  }

  /** Synchronous lifecycle-boundary checkpoint (page hide / native window close). */
  checkpoint() {
    this.persist();
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
    const principal = ports().chat.sessionId()?.trim() ?? "";
    for (const sessionId of nextIds) {
      if (sessionId !== principal) {
        void ports().chat.warmBackgroundSession(sessionId);
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

  bootstrap(scopeId = PERSONAL_WORKSPACE_SCOPE) {
    if (this.bootstrapped) return;
    this.bootstrapped = true;
    this.workspaceScopeId = scopeId.trim() || PERSONAL_WORKSPACE_SCOPE;

    const persisted = loadPersisted(this.workspaceScopeId);
    if (persisted) {
      const lme = ports().lme.restoreSession(persisted.lme);
      const lmeTabIds = new Set(lme.tabs.map((tab) => tab.tabId));
      this.desktops = persisted.desktops.map((desktop) => ({
        ...desktop,
        layout: filterLayoutForLme(desktop.layout, lmeTabIds),
      }));
      this.activeDesktopId = persisted.activeDesktopId;
      const activeDesktop =
        this.desktops.find((desktop) => desktop.id === persisted.activeDesktopId) ??
        this.desktops[0]!;
      this.applyLayout(activeDesktop.layout);
      if (this.tabs.length > 0) {
        // The restored shell tab is the startup source of truth. Activating it
        // rehydrates its transcript and updates the chat store; never synthesize
        // a blank tab from the standalone session key during bootstrap.
        const active = this.activeTab;
        if (active) {
          void this.activate(active.id, { rehydrate: true });
        }
        this.persist();
        return;
      }
    } else {
      this.ensureDesktopCatalog();
    }

    const surface = layout.desktopSurface;
    if (surface === "web") {
      const browserTab = ports().browser.activeTab();
      if (browserTab) {
        this.openWeb(browserTab.id, { activate: true });
        return;
      }
    }
    if (surface === "library" || surface === "automations" || surface === "code") {
      this.enterLmeFamily(surface === "code" ? "code" : "library");
      return;
    }
    if (surface === "chat") {
      const sessionId = ports().chat.sessionId()?.trim();
      if (sessionId) {
        this.openChat(sessionId, { activate: true });
      } else {
        this.openSurface("chat", { activate: true });
      }
      return;
    }
    if (isShellSurfaceTabId(surface) && surface !== "library") {
      this.openSurface(surface as Surface, { activate: true });
      return;
    }

    const sessionId = ports().chat.sessionId()?.trim();
    if (sessionId) {
      this.openChat(sessionId, { activate: true });
      return;
    }
    this.enterLmeFamily("library");
  }

  /**
   * Checkpoint one workshop and restore another only after its daemon is live.
   * Durable descriptors are workshop-scoped; live editor/Forge state is always
   * discarded at this boundary and rebuilt from the selected workshop.
   */
  async switchWorkspaceScope(scopeId: string) {
    const nextScope = scopeId.trim() || PERSONAL_WORKSPACE_SCOPE;
    if (!this.bootstrapped) {
      this.bootstrap(nextScope);
      return;
    }
    if (nextScope === this.workspaceScopeId) return;

    this.persist();
    const previousChatIds = this.chatSessionIdsForLiveRestore();
    this.workspaceScopeId = nextScope;
    this.navBackStack = [];
    this.navForwardStack = [];
    await disposeDestinationFeatures("workshop-switch");
    ports().code.resetForWorkshopSwitch();
    const { undertakings } = await import("$lib/stores/undertakings.svelte");
    undertakings.resetForWorkshopSwitch();
    const { mobileCodeWorkspaceState } = await import(
      "$lib/stores/mobileCodeWorkspaceState.svelte"
    );
    mobileCodeWorkspaceState.resetForWorkshopSwitch();

    const persisted = loadPersisted(nextScope);
    if (persisted) {
      const lme = ports().lme.restoreSession(persisted.lme);
      const lmeTabIds = new Set(lme.tabs.map((tab) => tab.tabId));
      this.desktops = persisted.desktops.map((desktop) => ({
        ...desktop,
        layout: filterLayoutForLme(desktop.layout, lmeTabIds),
      }));
      this.activeDesktopId = persisted.activeDesktopId;
      const activeDesktop =
        this.desktops.find((desktop) => desktop.id === persisted.activeDesktopId) ??
        this.desktops[0]!;
      this.applyLayout(activeDesktop.layout);
    } else {
      ports().lme.restoreSession({ tabs: [], activeTabId: null });
      const layout = emptyLayout();
      const id = newDesktopId();
      this.desktops = [{ id, name: DEFAULT_DESKTOP_NAME, layout }];
      this.activeDesktopId = id;
      this.applyLayout(layout);
    }

    const active = this.activeTab;
    if (active) {
      await this.activate(active.id, { rehydrate: true });
    } else {
      this.enterLmeFamily("library");
    }
    await this.resyncLiveStreams(previousChatIds);
    this.persist();
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

    const session = ports().chat.sessions().find((row) => row.session_id === trimmed);
    const messages = ports().chat.messagesFor(trimmed);
    const hasChatOrWorkerMessages = messages.some(
      (message) => isChatLaneMessage(message) || message.lane === "worker",
    );
    const title =
      options?.title?.trim() ||
      (session
        ? chatPresenceOrSessionLabel(session, { hasChatOrWorkerMessages })
        : presenceRoomTitle());
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
    const lmeTab = ports().lme.tabs().find((tab) => tab.tabId === trimmed);
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

    const placeholderSurfaceId = lmeTab?.kind === "code" ? "code" : "library";
    const librarySurface = this.tabs.find(
      (tab) => tab.kind === "surface" && tab.surfaceId === placeholderSurfaceId,
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
    const browserTab = ports().browser.tabs().find((tab) => tab.id === trimmed);
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

  openTerminal(
    sessionId: string,
    options?: {
      activate?: boolean;
      groupId?: string;
      title?: string;
      workId?: string | null;
    },
  ): string | null {
    const trimmed = sessionId.trim();
    if (!trimmed) return null;
    const groupId = options?.groupId ?? this.activeGroupId;
    const activate = options?.activate !== false;
    const existingInGroup = this.tabs.find(
      (tab) =>
        tab.kind === "terminal" &&
        tab.sessionId === trimmed &&
        this.groupForTab(tab.id)?.id === groupId,
    );
    if (existingInGroup) {
      if (options?.workId !== undefined && existingInGroup.kind === "terminal") {
        existingInGroup.workId = options.workId;
      }
      if (activate) void this.activate(existingInGroup.id);
      else this.persist();
      return existingInGroup.id;
    }
    const tab: ShellTab = {
      id: newTabId("terminal"),
      kind: "terminal",
      sessionId: trimmed,
      workId: options?.workId ?? null,
      title: options?.title?.trim() || "Terminal",
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
    if (next === "context") next = "map";
    if (
      next === "automations" ||
      next === "workshop" ||
      next === "notes" ||
      next === "files" ||
      next === "artifacts"
    ) {
      next = "library";
    }
    const groupId = options?.groupId ?? this.activeGroupId;
    if (next === "chat") {
      const focusedSessionChat = this.tabs.find(
        (tab) => tab.kind === "chat" && tab.sessionId === ports().chat.sessionId(),
      );
      const currentDesktopChat = focusedSessionChat ??
        [...this.tabs].reverse().find((tab) => tab.kind === "chat");
      if (currentDesktopChat?.kind === "chat") {
        const openOptions: { activate: boolean; groupId?: string } = {
          activate: options?.activate !== false,
        };
        if (options?.groupId !== undefined) openOptions.groupId = groupId;
        return this.openChat(currentDesktopChat.sessionId, openOptions);
      }
      const desktopId = this.activeDesktopId;
      const targetGroupId = groupId;
      void ports().chat.newSession({ shellContext: { desktopId, groupId: targetGroupId } });
      return null;
    }
    if (next === "web") {
      const browserTab = ports().browser.activeTab();
      if (browserTab) {
        return this.openWeb(browserTab.id, {
          activate: options?.activate !== false,
          groupId,
        });
      }
      void ports().browser.openTab("about:blank").then(() => {
        const created = ports().browser.activeTab();
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
    let next = surfaceId === "home" ? "chat" : surfaceId;
    if (next === "context") next = "map";
    if (
      next === "automations" ||
      next === "workshop" ||
      next === "notes" ||
      next === "files" ||
      next === "artifacts" ||
      next === "library"
    ) {
      this.enterLmeFamily("library");
      return;
    }
    if (next === "code") {
      this.enterLmeFamily("code");
      return;
    }
    this.openSurface(surfaceId, { activate: true });
  }

  /**
   * Enter Notes/Code/Automations without seeding empty Workspace/Code surface
   * tabs. Prefer an open document in that family; otherwise only update the
   * rail hint and leave the center pane alone.
   */
  enterLmeFamily(family: "library" | "code"): string | null {
    layout.focusDesktopSurface(family);

    const matchesFamily = (lmeTabId: string) => {
      const lme = ports().lme.tabs().find((tab) => tab.tabId === lmeTabId);
      if (!lme) return false;
      return family === "code" ? lme.kind === "code" : lme.kind !== "code";
    };

    const activateShellForLme = (lmeTabId: string, title?: string) => {
      const existing = this.tabs.find(
        (tab) => tab.kind === "lme" && tab.lmeTabId === lmeTabId,
      );
      if (existing) {
        if (title) this.patchTitle(existing.id, title);
        void this.activate(existing.id);
        return existing.id;
      }
      return this.openLme(lmeTabId, { activate: true, title });
    };

    const activeLme = ports().lme.activeTab();
    if (activeLme && matchesFamily(activeLme.tabId)) {
      return activateShellForLme(activeLme.tabId, activeLme.title);
    }

    const shellMatch = [...this.tabs]
      .reverse()
      .find((tab) => tab.kind === "lme" && matchesFamily(tab.lmeTabId));
    if (shellMatch && shellMatch.kind === "lme") {
      void this.activate(shellMatch.id);
      return shellMatch.id;
    }

    // No document in this family — keep whatever is focused (chat, etc.).
    return null;
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

  async activate(tabId: string, options?: { rehydrate?: boolean }) {
    const tab = this.tabs.find((entry) => entry.id === tabId);
    if (!tab) return;

    const previous = this.activeTab;
    const leavingLmeNote =
      previous &&
      previous.id !== tabId &&
      previous.kind === "lme" &&
      ports().lme.activeTab()?.kind === "note";
    if (leavingLmeNote) {
      // Flush vault drafts before shell remounts / swaps the active host.
      const ok = await ports().vault.flushBeforeLeave();
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
        // On cold restore the persisted ids already match, but the in-memory
        // transcript is still empty. Rehydrate must fetch that session anyway.
        if (options?.rehydrate || ports().chat.sessionId() !== tab.sessionId) {
          await ports().chat.switchSession(tab.sessionId);
        }
        return;
      }
      if (tab.kind === "lme") {
        if (options?.rehydrate || ports().lme.activeTabId() !== tab.lmeTabId) {
          await ports().lme.activateTab(tab.lmeTabId);
        } else {
          // Same LME tab (e.g. pane focus) — still promote vault focus if it drifted.
          const lme = ports().lme.tabs().find((entry) => entry.tabId === tab.lmeTabId);
          if (lme?.kind === "note" && !ports().vault.isFocusedPath(lme.path)) {
            await ports().vault.openNote(lme.path);
          }
        }
        return;
      }
      if (tab.kind === "web") {
        if (ports().browser.activeTab()?.id !== tab.browserTabId) {
          await ports().browser.activateTab(tab.browserTabId);
        }
      }
    } finally {
      this.endSuppressMirror();
    }
  }

  mirrorLmeTab(
    lmeTabId: string,
    options?: { activate?: boolean; title?: string; groupId?: string },
  ) {
    if (this.suppressMirror) return;
    this.openLme(lmeTabId, {
      activate: options?.activate !== false,
      title: options?.title,
      groupId: options?.groupId,
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
    if (tab.kind === "lme" && !ports().lme.confirmCloseTab(tab.lmeTabId)) {
      return;
    }
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
          void ports().lme.closeTab(tab.lmeTabId, {
            activateNext: false,
            confirmed: true,
          });
        }
      } else if (tab.kind === "web") {
        const stillOpen = this.tabs.some(
          (entry) => entry.kind === "web" && entry.browserTabId === tab.browserTabId,
        );
        if (!stillOpen) {
          void ports().browser.closeTab(tab.browserTabId);
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

  /** Replace the initial desktop with the chosen pane tree and no seeded tabs. */
  applyHomeOnboardingLayout(choice: HomeOnboardingLayout): void {
    const paneCount = choice === "focused" ? 1 : choice === "dashboard" ? 3 : 2;
    const groupIds = Array.from({ length: paneCount }, (_, index) =>
      index === 0 ? MAIN_GROUP_ID : newSplitId("group"),
    );
    const groups: EditorGroup[] = groupIds.map((id) => ({
      id,
      tabIds: [],
      activeTabId: null,
    }));

    let splitRoot: SplitNode = { type: "group", id: groupIds[0]! };
    if (paneCount >= 2) {
      splitRoot = {
        type: "branch",
        id: newSplitId("branch"),
        direction: "column",
        ratio: choice === "dashboard" ? 0.58 : 0.5,
        a: { type: "group", id: groupIds[0]! },
        b:
          paneCount === 3
            ? {
                type: "branch",
                id: newSplitId("branch"),
                direction: "row",
                ratio: 0.5,
                a: { type: "group", id: groupIds[1]! },
                b: { type: "group", id: groupIds[2]! },
              }
            : { type: "group", id: groupIds[1]! },
      };
    }

    this.applyLayout({
      tabs: [],
      groups,
      splitRoot,
      activeGroupId: groupIds[0]!,
      zoomedGroupId: null,
    });
    this.persist();
  }

  /**
   * Split and move the active tab into the new pane (workshop default).
   * Cloning into both panes remains available via `retainActiveInSplit`.
   */
  splitActive(direction: SplitDirection): boolean {
    return this.moveActiveToNewSplit(direction);
  }

  /** Split and move the active tab into the new pane. */
  moveActiveToNewSplit(direction: SplitDirection): boolean {
    return this.#splitWithSeed(direction, "move");
  }

  /** Split and retain (clone) the active tab in both panes — VS Code Split Editor. */
  retainActiveInSplit(direction: SplitDirection): boolean {
    return this.#splitWithSeed(direction, "retain");
  }

  #splitWithSeed(direction: SplitDirection, mode: "move" | "retain"): boolean {
    if (countLeaves(this.splitRoot) >= MAX_SHELL_PANES) return false;
    const fromGroupId = this.activeGroupId;
    const seed = this.activeTab;
    const newGroupId = newSplitId("group");
    const result = splitLeaf(this.splitRoot, fromGroupId, direction, newGroupId);
    if (!result) return false;
    this.splitRoot = result.root;
    this.groups = [...this.groups, { id: newGroupId, tabIds: [], activeTabId: null }];
    if (!seed) {
      this.activeGroupId = newGroupId;
      this.persist();
      return true;
    }
    if (mode === "move") {
      this.moveTab(seed.id, newGroupId);
      void this.activate(seed.id);
    } else {
      const retained = this.retainTabInGroup(seed, newGroupId);
      this.activeGroupId = newGroupId;
      if (retained) {
        this.patchGroup(newGroupId, { activeTabId: retained });
        void this.activate(retained);
      }
    }
    this.persist();
    return true;
  }

  /** Clone `seed` into `groupId` while leaving the original tab where it is. */
  private retainTabInGroup(seed: ShellTab, groupId: string): string | null {
    switch (seed.kind) {
      case "lme":
        return this.openLme(seed.lmeTabId, {
          activate: false,
          title: seed.title,
          groupId,
        });
      case "chat":
        return this.openChat(seed.sessionId, {
          activate: false,
          title: seed.title,
          groupId,
        });
      case "web":
        return this.openWeb(seed.browserTabId, {
          activate: false,
          title: seed.title,
          groupId,
        });
      case "surface":
        return this.openSurface(seed.surfaceId, { activate: false, groupId });
      case "terminal":
        return this.openTerminal(seed.sessionId, {
          activate: false,
          title: seed.title,
          groupId,
          workId: seed.workId,
        });
      default:
        return null;
    }
  }

  /** Split `hostGroupId` toward `edge` and move `tabId` into the new pane. */
  splitGroupWithTab(hostGroupId: string, tabId: string, edge: SplitEdge): boolean {
    if (countLeaves(this.splitRoot) >= MAX_SHELL_PANES) return false;
    if (!this.tabs.some((tab) => tab.id === tabId)) return false;
    if (!this.groups.some((group) => group.id === hostGroupId)) return false;

    const newGroupId = newSplitId("group");
    const result = splitLeafAtEdge(this.splitRoot, hostGroupId, edge, newGroupId);
    if (!result) return false;

    this.splitRoot = result.root;
    this.groups = [...this.groups, { id: newGroupId, tabIds: [], activeTabId: null }];
    this.moveTab(tabId, newGroupId);
    void this.activate(tabId);
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

  /** Close the active pane and merge its tabs into the sash-adjacent sibling. */
  closeActiveGroup(): boolean {
    if (countLeaves(this.splitRoot) <= 1) return false;
    const closingId = this.activeGroupId;
    const targetId = mergeTargetForLeaf(this.splitRoot, closingId);
    if (!targetId) return false;

    const closing = this.groups.find((group) => group.id === closingId);
    const tabIds = [...(closing?.tabIds ?? [])];
    const focusTabId = closing?.activeTabId ?? tabIds[tabIds.length - 1] ?? null;

    for (const tabId of tabIds) {
      this.moveTab(tabId, targetId);
    }

    const result = removeLeaf(this.splitRoot, closingId);
    if (!result.removed) return false;

    this.groups = this.groups.filter((group) => group.id !== closingId);
    this.splitRoot = result.root;
    if (this.zoomedGroupId === closingId) {
      this.zoomedGroupId = null;
    }
    this.activeGroupId = targetId;
    if (focusTabId && this.tabs.some((tab) => tab.id === focusTabId)) {
      void this.activate(focusTabId);
    } else {
      const active = this.activeGroup;
      if (active.activeTabId) {
        void this.activate(active.activeTabId);
      } else {
        this.syncLayoutHint(null);
      }
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

  /** Cycle Code file LME tabs in the focused pane (Ctrl+Tab muscle memory). */
  cycleCodeSourceTabsInActiveGroup(delta = 1) {
    const tabs = this.tabsForGroup(this.activeGroupId).filter((tab) => {
      if (tab.kind !== "lme") return false;
      const lme = ports().lme.tabs().find((entry) => entry.tabId === tab.lmeTabId);
      return lme?.kind === "code" && lme.resource.kind === "file";
    });
    if (tabs.length < 2) {
      if (delta > 0) this.nextTabInActiveGroup();
      else this.prevTabInActiveGroup();
      return;
    }
    const idx = tabs.findIndex((tab) => tab.id === this.activeTabId);
    const start = idx < 0 ? 0 : idx;
    const next = tabs[(start + delta + tabs.length) % tabs.length];
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
    if (!this.bootstrapped) return;
    let changed = false;
    const next = this.tabs.map((tab) => {
      if (tab.kind === "chat") {
        const session = ports().chat.sessions().find((row) => row.session_id === tab.sessionId);
        if (!session) return tab;
        const messages = ports().chat.messagesFor(tab.sessionId);
        const hasChatOrWorkerMessages = messages.some(
          (message) => isChatLaneMessage(message) || message.lane === "worker",
        );
        // While hydrating an empty buffer, keep the existing tab title so we
        // don't flash a Presence label over a session that still has turns.
        if (!hasChatOrWorkerMessages && ports().chat.historyLoadingFor(tab.sessionId)) {
          return tab;
        }
        const title = chatPresenceOrSessionLabel(session, {
          hasChatOrWorkerMessages,
        });
        if (title !== tab.title) {
          changed = true;
          return { ...tab, title };
        }
        return tab;
      }
      if (tab.kind === "lme") {
        const lme = ports().lme.tabs().find((entry) => entry.tabId === tab.lmeTabId);
        if (!lme) return tab;
        const title = lme.title?.trim() || tab.title;
        if (title !== tab.title) {
          changed = true;
          return { ...tab, title };
        }
        return tab;
      }
      if (tab.kind === "web") {
        const browserTab = ports().browser.tabs().find((entry) => entry.id === tab.browserTabId);
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
    if (!this.bootstrapped) return;
    const lmeIds = new Set(ports().lme.tabs().map((tab) => tab.tabId));
    // LME's document catalog is global, but shell presentations belong to a
    // desktop. Do not mirror a document into the active desktop merely because
    // the global LME active tab changed; it may already be presented elsewhere.
    const presentedLmeIds = new Set<string>();
    const collectPresented = (tabs: ShellTab[]) => {
      for (const tab of tabs) {
        if (tab.kind === "lme") presentedLmeIds.add(tab.lmeTabId);
      }
    };
    collectPresented(this.tabs);
    for (const desktop of this.desktops) {
      collectPresented(desktop.layout.tabs);
    }
    for (const lme of ports().lme.tabs()) {
      const existing = this.tabs.find(
        (tab) => tab.kind === "lme" && tab.lmeTabId === lme.tabId,
      );
      if (!existing) {
        if (!presentedLmeIds.has(lme.tabId)) {
          // Activate when this is the LME focus so ShellTabHost sync does not
          // leave the new note as a background chip ahead of mirrorLmeTab.
          const shouldActivate = lme.tabId === ports().lme.activeTabId();
          this.openLme(lme.tabId, {
            activate: shouldActivate,
            title: lme.title,
          });
          presentedLmeIds.add(lme.tabId);
        }
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
    if (!this.bootstrapped) return;
    const browserIds = new Set(ports().browser.tabs().map((tab) => tab.id));
    const hasWebShell = this.tabs.some((tab) => tab.kind === "web");
    const webEngaged =
      hasWebShell ||
      this.activeTab?.kind === "web" ||
      layout.desktopSurface === "web";

    if (webEngaged) {
      for (const browserTab of ports().browser.tabs()) {
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

  createDesktop(name?: string, options?: { activate?: boolean }): string {
    this.ensureDesktopCatalog();
    if (this.desktops.length >= MAX_SHELL_DESKTOPS) return "";
    this.flushActiveDesktop();
    const trimmed = name?.trim() || `Desktop ${this.desktops.length + 1}`;
    const id = newDesktopId();
    this.desktops = [
      ...this.desktops,
      { id, name: trimmed, layout: emptyLayout() },
    ];
    this.persist();
    if (options?.activate !== false) {
      void this.switchDesktop(id);
    }
    return id;
  }

  /** Move a live tab onto another virtual desktop's focused pane. */
  moveTabToDesktop(tabId: string, desktopId: string): boolean {
    this.ensureDesktopCatalog();
    const trimmedDesktop = desktopId.trim();
    if (!trimmedDesktop || trimmedDesktop === this.activeDesktopId) return false;
    const tab = this.tabs.find((entry) => entry.id === tabId);
    if (!tab) return false;
    if (!this.desktops.some((desktop) => desktop.id === trimmedDesktop)) return false;

    this.removeTabFromAllGroups(tabId);
    this.tabs = this.tabs.filter((entry) => entry.id !== tabId);
    this.pruneNavStacks();

    this.desktops = this.desktops.map((desktop) =>
      desktop.id === trimmedDesktop
        ? { ...desktop, layout: appendTabsToLayout(desktop.layout, [tab]) }
        : desktop,
    );
    this.persist();
    return true;
  }

  /** Move every tab in a pane onto another desktop, then drop the empty pane. */
  movePaneToDesktop(groupId: string, desktopId: string): boolean {
    this.ensureDesktopCatalog();
    const trimmedDesktop = desktopId.trim();
    if (!trimmedDesktop || trimmedDesktop === this.activeDesktopId) return false;
    if (!this.desktops.some((desktop) => desktop.id === trimmedDesktop)) return false;

    const group = this.groups.find((entry) => entry.id === groupId);
    if (!group) return false;

    const moved = group.tabIds
      .map((id) => this.tabs.find((tab) => tab.id === id))
      .filter((tab): tab is ShellTab => Boolean(tab));

    for (const tab of moved) {
      this.removeTabFromAllGroups(tab.id);
    }
    const movedIds = new Set(moved.map((tab) => tab.id));
    this.tabs = this.tabs.filter((tab) => !movedIds.has(tab.id));
    this.pruneNavStacks();

    this.desktops = this.desktops.map((desktop) =>
      desktop.id === trimmedDesktop
        ? { ...desktop, layout: appendTabsToLayout(desktop.layout, moved) }
        : desktop,
    );

    if (countLeaves(this.splitRoot) > 1) {
      const result = removeLeaf(this.splitRoot, groupId);
      if (result.removed) {
        this.splitRoot = result.root;
        this.groups = this.groups.filter((entry) => entry.id !== groupId);
        if (this.zoomedGroupId === groupId) this.zoomedGroupId = null;
        if (this.activeGroupId === groupId) {
          const remaining = collectGroupIds(this.splitRoot);
          this.activeGroupId = remaining[remaining.length - 1] ?? MAIN_GROUP_ID;
        }
      }
    }

    const active = this.activeGroup;
    if (active.activeTabId) {
      void this.activate(active.activeTabId);
    } else {
      this.syncLayoutHint(null);
    }
    this.persist();
    return true;
  }

  /** Switch by 0-based catalog index (Ctrl+; 1–4). No-op if index is empty. */
  async switchDesktopAt(index: number): Promise<boolean> {
    this.ensureDesktopCatalog();
    if (!Number.isInteger(index) || index < 0 || index >= this.desktops.length) {
      return false;
    }
    const target = this.desktops[index];
    if (!target) return false;
    return this.switchDesktop(target.id);
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
      await this.activate(active.id, { rehydrate: true });
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
        await this.activate(active.id, { rehydrate: true });
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

  /** Every open tab across virtual desktops (active desktop uses live state). */
  collectSearchHits(): ShellTabSearchHit[] {
    this.ensureDesktopCatalog();
    const hits: ShellTabSearchHit[] = [];
    const liveActiveId = this.activeTabId;

    for (const desktop of this.desktops) {
      const isActiveDesktop = desktop.id === this.activeDesktopId;
      const tabs = isActiveDesktop ? this.tabs : desktop.layout.tabs;
      const groups = isActiveDesktop ? this.groups : desktop.layout.groups;
      const splitRoot = isActiveDesktop ? this.splitRoot : desktop.layout.splitRoot;
      const order = leafOrder(splitRoot);
      const byId = new Map(tabs.map((tab) => [tab.id, tab]));

      for (const groupId of order) {
        const group = groups.find((entry) => entry.id === groupId);
        if (!group) continue;
        const paneIndex = order.indexOf(groupId) + 1;
        for (const tabId of group.tabIds) {
          const tab = byId.get(tabId);
          if (!tab) continue;
          hits.push({
            tabId: tab.id,
            title: titleOfTab(tab),
            kind: tab.kind,
            desktopId: desktop.id,
            desktopName: desktop.name,
            groupId,
            paneIndex,
            isActive: isActiveDesktop && tab.id === liveActiveId,
            isActiveDesktop,
          });
        }
      }
    }
    return hits;
  }

  /** Switch desktop if needed, then focus the tab's pane. */
  async revealSearchHit(desktopId: string, tabId: string): Promise<boolean> {
    const trimmedDesktop = desktopId.trim();
    const trimmedTab = tabId.trim();
    if (!trimmedDesktop || !trimmedTab) return false;

    if (trimmedDesktop !== this.activeDesktopId) {
      const switched = await this.switchDesktop(trimmedDesktop);
      if (!switched) return false;
    }
    if (!this.tabs.some((tab) => tab.id === trimmedTab)) return false;
    await this.activate(trimmedTab);
    return true;
  }
}

export const shellTabs = new ShellTabsStore();
