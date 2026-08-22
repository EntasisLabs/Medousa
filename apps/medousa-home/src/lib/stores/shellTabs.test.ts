import { beforeEach, describe, expect, it, vi } from "vitest";

const layoutState = {
  desktopSurface: "chat" as string,
  focusDesktopSurface: vi.fn((surface: string) => {
    layoutState.desktopSurface = surface;
  }),
  setShellSidebarMode: vi.fn(),
  toggleShellSidebarExpanded: vi.fn(),
};

vi.mock("$lib/stores/chat.svelte", () => ({
  chat: {
    sessionId: "session-a",
    sessions: [
      {
        session_id: "session-a",
        display_name: "Alpha",
        preview: "Alpha",
        last_timestamp: null,
      },
      {
        session_id: "session-b",
        display_name: "Beta",
        preview: "Beta",
        last_timestamp: null,
      },
    ],
    messagesFor: vi.fn(() => []),
    historyLoadingFor: vi.fn(() => false),
    warmBackgroundSession: vi.fn(),
    switchSession: vi.fn(async function (this: { sessionId: string }, id: string) {
      this.sessionId = id;
    }),
    newSession: vi.fn(async () => {}),
  },
}));

vi.mock("$lib/chat/chatStreamPool.svelte", () => ({
  chatStreamPool: {
    acquire: vi.fn(),
    release: vi.fn(),
    isLive: vi.fn(() => true),
  },
}));

const lmeState = {
  tabs: [] as Array<Record<string, any>>,
  activeTabId: null as string | null,
  get activeTab() {
    return this.tabs.find((tab) => tab.tabId === this.activeTabId) ?? null;
  },
  activateTab: vi.fn(async () => {}),
  closeTab: vi.fn(async () => {}),
  confirmCloseTab: vi.fn(() => true),
  captureSession() {
    return { tabs: this.tabs, activeTabId: this.activeTabId };
  },
  restoreSession(value: unknown) {
    const session = value as {
      tabs?: Array<Record<string, any>>;
      activeTabId?: string | null;
    };
    this.tabs = session?.tabs ?? [];
    this.activeTabId = session?.activeTabId ?? null;
    return { tabs: this.tabs, activeTabId: this.activeTabId };
  },
};

vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({
  lmeWorkspace: lmeState,
}));

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    flushBeforeLeave: vi.fn(async () => true),
    openNote: vi.fn(async () => {}),
    isFocusedPath: vi.fn(() => true),
  },
}));

vi.mock("$lib/stores/humanBrowser.svelte", () => ({
  humanBrowser: {
    tabs: [],
    activeTab: null,
    activateTab: vi.fn(async () => {}),
    closeTab: vi.fn(async () => {}),
    openTab: vi.fn(async () => {}),
  },
}));

vi.mock("$lib/runtime/layout.svelte", () => ({
  layout: layoutState,
}));

vi.mock("$lib/stores/codeWorkspace.svelte", () => ({
  codeWorkspace: {
    resetForWorkshopSwitch: vi.fn(),
    tabsFor: vi.fn(() => []),
    hydrate: vi.fn(async () => {}),
    open: vi.fn(async () => null),
  },
}));

async function loadShellTabs() {
  const { setShellTabPorts } = await import("$lib/runtime/shellTabPorts");
  const { chat } = await import("$lib/stores/chat.svelte");
  const { vault } = await import("$lib/stores/vault.svelte");
  const { humanBrowser } = await import("$lib/stores/humanBrowser.svelte");
  const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
  const { codeWorkspace } = await import("$lib/stores/codeWorkspace.svelte");
  setShellTabPorts({
    chat: {
      sessionId: () => chat.sessionId,
      sessions: () => chat.sessions,
      messagesFor: (sessionId) => chat.messagesFor(sessionId),
      historyLoadingFor: (sessionId) => chat.historyLoadingFor(sessionId),
      warmBackgroundSession: (sessionId) => {
        chat.warmBackgroundSession(sessionId);
      },
      switchSession: (sessionId) => chat.switchSession(sessionId),
      newSession: (options) => {
        void chat.newSession(options);
      },
    },
    lme: {
      tabs: () => lmeWorkspace.tabs as never,
      activeTab: () => lmeWorkspace.activeTab as never,
      activeTabId: () => lmeWorkspace.activeTabId,
      captureSession: () => lmeWorkspace.captureSession() as never,
      restoreSession: (value) => lmeWorkspace.restoreSession(value) as never,
      activateTab: (tabId) => lmeWorkspace.activateTab(tabId),
      closeTab: (tabId, options) => lmeWorkspace.closeTab(tabId, options),
      confirmCloseTab: (tabId) => lmeWorkspace.confirmCloseTab(tabId),
    },
    vault: {
      flushBeforeLeave: () => vault.flushBeforeLeave(),
      openNote: (path) => vault.openNote(path),
      isFocusedPath: (path) => vault.isFocusedPath(path),
    },
    browser: {
      tabs: () => humanBrowser.tabs,
      activeTab: () => humanBrowser.activeTab,
      activateTab: (tabId) => humanBrowser.activateTab(tabId),
      closeTab: (tabId) => {
        void humanBrowser.closeTab(tabId);
      },
      openTab: async (url) => {
        await humanBrowser.openTab(url);
      },
    },
    code: {
      resetForWorkshopSwitch: () => codeWorkspace.resetForWorkshopSwitch(),
    },
  });
  return import("./shellTabs.svelte");
}

describe("shellTabs store", () => {
  beforeEach(() => {
    vi.resetModules();
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => {
        store.set(key, value);
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => store.clear(),
    });
    layoutState.desktopSurface = "chat";
    layoutState.focusDesktopSurface.mockClear();
    lmeState.tabs = [];
    lmeState.activeTabId = null;
    lmeState.activateTab.mockClear();
    lmeState.closeTab.mockClear();
  });

  it("applies onboarding pane shapes without seeding tabs", async () => {
    const { shellTabs } = await loadShellTabs();

    for (const [choice, panes] of [
      ["focused", 1],
      ["split", 2],
      ["dashboard", 3],
    ] as const) {
      shellTabs.openChat("session-a", { activate: true });
      shellTabs.applyHomeOnboardingLayout(choice);

      expect(shellTabs.paneCount).toBe(panes);
      expect(shellTabs.tabs).toHaveLength(0);
      expect(shellTabs.groups).toHaveLength(panes);
      expect(shellTabs.groups.every((group) =>
        group.tabIds.length === 0 && group.activeTabId === null
      )).toBe(true);
    }
  });

  it("opens chat tabs uniquely per group and activates", async () => {
    const { shellTabs } = await loadShellTabs();
    const a = shellTabs.openChat("session-a", { activate: true });
    const b = shellTabs.openChat("session-b", { activate: true });
    const again = shellTabs.openChat("session-a", { activate: true });

    expect(a).toBeTruthy();
    expect(b).toBeTruthy();
    expect(again).toBe(a);
    expect(shellTabs.orderedTabs).toHaveLength(2);
    expect(shellTabs.activeTab?.kind).toBe("chat");
    if (shellTabs.activeTab?.kind === "chat") {
      expect(shellTabs.activeTab.sessionId).toBe("session-a");
    }
  });

  it("returns to the focused chat session instead of the first chat tab", async () => {
    const { shellTabs } = await loadShellTabs();
    const { chat } = await import("$lib/stores/chat.svelte");
    shellTabs.openChat("session-a", { activate: true });
    const latest = shellTabs.openChat("session-b", { activate: true });
    await vi.waitFor(() => expect(chat.sessionId).toBe("session-b"));
    shellTabs.openSurface("map", { activate: true });

    expect(shellTabs.openSurface("chat", { activate: true })).toBe(latest);
    expect(shellTabs.activeTab).toMatchObject({
      kind: "chat",
      sessionId: "session-b",
    });
  });

  it("keeps governed terminal ownership on the shell tab", async () => {
    const { shellTabs } = await loadShellTabs();
    const tabId = shellTabs.openTerminal("pty-a", {
      activate: true,
      title: "Terminal · Refactor auth",
      workId: "work-a",
    });

    expect(tabId).toBeTruthy();
    expect(shellTabs.activeTab?.kind).toBe("terminal");
    if (shellTabs.activeTab?.kind === "terminal") {
      expect(shellTabs.activeTab.workId).toBe("work-a");
      expect(shellTabs.activeTab.title).toBe("Terminal · Refactor auth");
    }
  });

  it("splits into a second pane", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.groups).toHaveLength(2);
  });

  it("splits by moving the active tab into the new pane", async () => {
    const { shellTabs } = await loadShellTabs();
    lmeState.tabs = [
      { tabId: "lme-1", kind: "note", path: "notes/split.md", title: "Split note" },
    ];
    lmeState.activeTabId = "lme-1";
    const shellId = shellTabs.openLme("lme-1", { activate: true, title: "Split note" });
    expect(shellId).toBeTruthy();
    const fromGroupId = shellTabs.activeGroupId;
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.activeGroupId).not.toBe(fromGroupId);
    expect(shellTabs.tabs.filter((tab) => tab.kind === "lme")).toHaveLength(1);
    const from = shellTabs.groups.find((group) => group.id === fromGroupId);
    const to = shellTabs.groups.find((group) => group.id === shellTabs.activeGroupId);
    expect(from?.tabIds).not.toContain(shellId);
    expect(to?.tabIds).toContain(shellId);
  });

  it("can retain the active tab in both panes via retainActiveInSplit", async () => {
    const { shellTabs } = await loadShellTabs();
    lmeState.tabs = [
      { tabId: "lme-retain", kind: "note", path: "notes/retain.md", title: "Retain note" },
    ];
    lmeState.activeTabId = "lme-retain";
    const shellId = shellTabs.openLme("lme-retain", {
      activate: true,
      title: "Retain note",
    });
    expect(shellId).toBeTruthy();
    const fromGroupId = shellTabs.activeGroupId;
    expect(shellTabs.retainActiveInSplit("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    const from = shellTabs.groups.find((group) => group.id === fromGroupId);
    const to = shellTabs.groups.find((group) => group.id === shellTabs.activeGroupId);
    expect(from?.tabIds).toContain(shellId);
    expect(to?.tabIds.some((id) => id !== shellId)).toBe(true);
    expect(
      shellTabs.tabs.filter(
        (tab) => tab.kind === "lme" && tab.lmeTabId === "lme-retain",
      ),
    ).toHaveLength(2);
  });

  it("moves a focused note out when splitting a chat+note group", async () => {
    const { shellTabs } = await loadShellTabs();
    lmeState.tabs = [
      { tabId: "lme-mix", kind: "note", path: "notes/mix.md", title: "Mix note" },
    ];
    lmeState.activeTabId = "lme-mix";
    const chatId = shellTabs.openChat("session-mix", { activate: true, title: "Chat" });
    const noteId = shellTabs.openLme("lme-mix", { activate: true, title: "Mix note" });
    expect(chatId).toBeTruthy();
    expect(noteId).toBeTruthy();
    const fromGroupId = shellTabs.activeGroupId;
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    const from = shellTabs.groups.find((group) => group.id === fromGroupId);
    const to = shellTabs.groups.find((group) => group.id === shellTabs.activeGroupId);
    expect(from?.tabIds).toContain(chatId);
    expect(from?.tabIds).not.toContain(noteId);
    expect(from?.activeTabId).toBe(chatId);
    expect(to?.tabIds).toContain(noteId);
    expect(shellTabs.tabs.filter((tab) => tab.kind === "lme")).toHaveLength(1);
  });

  it("can move the active tab into a new split as a separate command", async () => {
    const { shellTabs } = await loadShellTabs();
    lmeState.tabs = [
      { tabId: "lme-2", kind: "note", path: "notes/move.md", title: "Move note" },
    ];
    lmeState.activeTabId = "lme-2";
    const shellId = shellTabs.openLme("lme-2", { activate: true, title: "Move note" });
    expect(shellId).toBeTruthy();
    const fromGroupId = shellTabs.activeGroupId;
    expect(shellTabs.moveActiveToNewSplit("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    const from = shellTabs.groups.find((group) => group.id === fromGroupId);
    const to = shellTabs.groups.find((group) => group.id === shellTabs.activeGroupId);
    expect(from?.tabIds).not.toContain(shellId);
    expect(to?.tabIds).toContain(shellId);
    expect(shellTabs.tabs.filter((tab) => tab.kind === "lme")).toHaveLength(1);
  });

  it("splits a host pane with a dragged tab on an edge", async () => {
    const { shellTabs } = await loadShellTabs();
    const hostId = shellTabs.openChat("session-a", { activate: true });
    expect(hostId).toBeTruthy();
    const guestId = shellTabs.openChat("session-b", { activate: true });
    expect(guestId).toBeTruthy();
    const hostGroupId = shellTabs.activeGroupId;
    expect(shellTabs.splitGroupWithTab(hostGroupId, guestId!, "bottom")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.activeTab?.id).toBe(guestId);
    expect(shellTabs.splitRoot.type).toBe("branch");
    if (shellTabs.splitRoot.type === "branch") {
      expect(shellTabs.splitRoot.direction).toBe("row");
      expect(shellTabs.splitRoot.b.type).toBe("group");
    }
  });

  it("refuses a fifth pane", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(true);
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(false);
    expect(shellTabs.paneCount).toBe(4);
  });

  it("moves a tab to another desktop", async () => {
    const { shellTabs } = await loadShellTabs();
    const tabId = shellTabs.openChat("session-a", { activate: true });
    expect(tabId).toBeTruthy();
    const otherId = shellTabs.createDesktop("Staging", { activate: false });
    expect(otherId).toBeTruthy();
    expect(shellTabs.moveTabToDesktop(tabId!, otherId)).toBe(true);
    expect(shellTabs.tabs.some((tab) => tab.id === tabId)).toBe(false);
    const staging = shellTabs.desktops.find((desktop) => desktop.id === otherId);
    expect(staging?.layout.tabs.some((tab) => tab.id === tabId)).toBe(true);
  });

  it("moves a pane's tabs to another desktop and drops the pane", async () => {
    const { shellTabs } = await loadShellTabs();
    const stayId = shellTabs.openChat("session-a", { activate: true });
    const moveId = shellTabs.openChat("session-b", { activate: true });
    expect(stayId).toBeTruthy();
    expect(moveId).toBeTruthy();
    // splitActive moves the focused tab; moveId goes to the new pane and
    // stayId remains alone in the source pane.
    expect(shellTabs.splitActive("right")).toBe(true);
    const rightGroup = shellTabs.activeGroupId;
    expect(shellTabs.paneCount).toBe(2);
    const rightTabId = shellTabs.groups.find((group) => group.id === rightGroup)
      ?.activeTabId;
    expect(rightTabId).toBe(moveId);

    const otherId = shellTabs.createDesktop("Park", { activate: false });
    expect(shellTabs.movePaneToDesktop(rightGroup, otherId)).toBe(true);
    expect(shellTabs.paneCount).toBe(1);
    expect(shellTabs.tabs.some((tab) => tab.id === rightTabId)).toBe(false);
    expect(shellTabs.tabs.some((tab) => tab.id === stayId)).toBe(true);
    expect(shellTabs.tabs.some((tab) => tab.id === moveId)).toBe(false);
    const park = shellTabs.desktops.find((desktop) => desktop.id === otherId);
    expect(park?.layout.tabs.some((tab) => tab.id === rightTabId)).toBe(true);
  });

  it("collects search hits across desktops and reveals them", async () => {
    const { shellTabs } = await loadShellTabs();
    const mainTab = shellTabs.openChat("session-a", { activate: true });
    expect(mainTab).toBeTruthy();
    const otherDesktop = shellTabs.createDesktop("Research");
    expect(otherDesktop).toBeTruthy();
    const researchTab = shellTabs.openChat("session-b", { activate: true });
    expect(researchTab).toBeTruthy();

    const hits = shellTabs.collectSearchHits();
    expect(hits.some((hit) => hit.tabId === mainTab && hit.desktopName === "Main")).toBe(true);
    expect(
      hits.some((hit) => hit.tabId === researchTab && hit.desktopName === "Research"),
    ).toBe(true);

    expect(await shellTabs.revealSearchHit(
      hits.find((hit) => hit.tabId === mainTab)!.desktopId,
      mainTab!,
    )).toBe(true);
    expect(shellTabs.activeTab?.id).toBe(mainTab);
    expect(shellTabs.activeDesktopName).toBe("Main");
  });

  it("closes a pane by merging tabs into the sibling", async () => {
    const { shellTabs } = await loadShellTabs();
    const tabId = shellTabs.openChat("session-a", { activate: true });
    expect(tabId).toBeTruthy();
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    // Move split: the only chat tab lives in the new pane; source is empty.
    expect(
      shellTabs.tabs.filter(
        (tab) => tab.kind === "chat" && tab.sessionId === "session-a",
      ),
    ).toHaveLength(1);
    expect(shellTabs.activeTab?.id).toBe(tabId);
    expect(shellTabs.closeActiveGroup()).toBe(true);
    expect(shellTabs.paneCount).toBe(1);
    expect(
      shellTabs.tabs.filter(
        (tab) => tab.kind === "chat" && tab.sessionId === "session-a",
      ),
    ).toHaveLength(1);
    expect(shellTabs.closeActiveGroup()).toBe(false);
  });

  it("opens singleton surface tabs once", async () => {
    const { shellTabs } = await loadShellTabs();
    const first = shellTabs.openSurface("peers", { activate: true });
    const second = shellTabs.openSurface("peers", { activate: true });
    expect(first).toBe(second);
    expect(shellTabs.orderedTabs.filter((tab) => tab.kind === "surface")).toHaveLength(1);
  });

  it("keeps editor groups shaped for splits", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.openSurface("map", { activate: true });
    expect(shellTabs.groups.length).toBeGreaterThanOrEqual(1);
    expect(shellTabs.splitRoot.type).toBe("group");
  });

  it("persists and restores split layout across bootstrap", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    const branchId =
      shellTabs.splitRoot.type === "branch" ? shellTabs.splitRoot.id : null;
    expect(branchId).toBeTruthy();
    if (branchId) shellTabs.setRatio(branchId, 0.35);
    shellTabs.zoomToggle();
    const zoomed = shellTabs.zoomedGroupId;
    const activeGroup = shellTabs.activeGroupId;
    const ratio =
      shellTabs.splitRoot.type === "branch" ? shellTabs.splitRoot.ratio : null;
    expect(ratio).toBeCloseTo(0.35);
    expect(shellTabs.activeDesktopName).toBe("Main");

    vi.resetModules();
    const { shellTabs: restored } = await loadShellTabs();
    const { chat } = await import("$lib/stores/chat.svelte");
    vi.mocked(chat.switchSession).mockClear();
    restored.bootstrap();
    await vi.waitFor(() => {
      expect(chat.switchSession).toHaveBeenCalledWith("session-a");
    });
    expect(restored.paneCount).toBe(2);
    expect(restored.activeGroupId).toBe(activeGroup);
    expect(restored.zoomedGroupId).toBe(zoomed);
    expect(restored.splitRoot.type).toBe("branch");
    if (restored.splitRoot.type === "branch") {
      expect(restored.splitRoot.ratio).toBeCloseTo(0.35);
    }
    expect(restored.chatSessionIdsForLiveRestore()).toContain("session-a");
    expect(restored.activeDesktopName).toBe("Main");
  });

  it("keeps the restored chat tab authoritative over a stale session key", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    shellTabs.openChat("session-a", { activate: true });

    vi.resetModules();
    const { chat } = await import("$lib/stores/chat.svelte");
    chat.sessionId = "session-new";
    const { shellTabs: restored } = await loadShellTabs();
    restored.bootstrap();

    expect(restored.activeTab).toMatchObject({
      kind: "chat",
      sessionId: "session-a",
    });
    await vi.waitFor(() => {
      expect(chat.switchSession).toHaveBeenCalledWith("session-a");
    });
  });

  it("isolates durable workspace sessions by workshop", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap("personal");
    shellTabs.openSurface("map", { activate: true });
    expect(shellTabs.activeTab).toMatchObject({ kind: "surface", surfaceId: "map" });

    await shellTabs.switchWorkspaceScope("portal-team");
    expect(shellTabs.tabs.some((tab) => tab.kind === "surface" && tab.surfaceId === "map"))
      .toBe(false);
    shellTabs.openSurface("settings", { activate: true });

    await shellTabs.switchWorkspaceScope("personal");
    expect(shellTabs.tabs.some((tab) => tab.kind === "surface" && tab.surfaceId === "map"))
      .toBe(true);
    expect(shellTabs.tabs.some((tab) => tab.kind === "surface" && tab.surfaceId === "settings"))
      .toBe(false);
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toBeTruthy();
    expect(localStorage.getItem("medousa-home-workspace-session-v4:portal-team")).toBeTruthy();
  });

  it("restores shell and code workspace descriptors from one snapshot", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    const lmeTab = {
      tabId: "code-file:work-a:src%2Flib.rs",
      kind: "code",
      workId: "work-a",
      title: "lib.rs",
      resource: { kind: "file", path: "src/lib.rs", line: 42 },
    };
    lmeState.tabs = [lmeTab];
    lmeState.activeTabId = lmeTab.tabId;
    const shellId = shellTabs.openLme(lmeTab.tabId, {
      activate: true,
      title: lmeTab.title,
    });
    expect(shellId).toBeTruthy();
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toBeTruthy();

    vi.resetModules();
    lmeState.tabs = [];
    lmeState.activeTabId = null;
    const { shellTabs: restored } = await loadShellTabs();
    restored.bootstrap();

    expect(lmeState.tabs).toEqual([lmeTab]);
    expect(lmeState.activeTabId).toBe(lmeTab.tabId);
    expect(restored.activeTab).toMatchObject({
      kind: "lme",
      lmeTabId: lmeTab.tabId,
      title: "lib.rs",
    });
    expect(lmeState.activateTab).toHaveBeenCalledWith(lmeTab.tabId);
  });

  it("does not wipe a saved session when LME sync runs before bootstrap", async () => {
    const lmeTab = {
      tabId: "code-file:work-a:src%2Flib.rs",
      kind: "code",
      workId: "work-a",
      title: "lib.rs",
      resource: { kind: "file", path: "src/lib.rs", line: 1 },
    };
    localStorage.setItem(
      "medousa-home-workspace-session-v4:personal",
      JSON.stringify({
        version: 4,
        savedAt: Date.now(),
        activeDesktopId: "desk-1",
        desktops: [{
          id: "desk-1",
          name: "Main",
          layout: {
            tabs: [{
              id: "shell-code",
              kind: "lme",
              lmeTabId: lmeTab.tabId,
              title: "lib.rs",
            }],
            groups: [{ id: "main", tabIds: ["shell-code"], activeTabId: "shell-code" }],
            splitRoot: { type: "group", id: "main" },
            activeGroupId: "main",
            zoomedGroupId: null,
          },
        }],
        lme: { tabs: [lmeTab], activeTabId: lmeTab.tabId },
      }),
    );

    const { shellTabs } = await loadShellTabs();
    // Simulate ShellTabHost effects firing before onMount bootstrap.
    lmeState.tabs = [];
    lmeState.activeTabId = null;
    shellTabs.syncFromLmeWorkspace();
    shellTabs.syncTitlesFromStores();

    const raw = localStorage.getItem("medousa-home-workspace-session-v4:personal");
    expect(raw).toBeTruthy();
    expect(JSON.parse(raw!).lme.tabs).toEqual([lmeTab]);

    shellTabs.bootstrap();
    expect(lmeState.tabs).toEqual([lmeTab]);
    expect(shellTabs.activeTab).toMatchObject({
      kind: "lme",
      lmeTabId: lmeTab.tabId,
    });
  });

  it("migrates v2 layout into a Main desktop", async () => {
    const v2 = {
      tabs: [
        {
          id: "chat-1",
          kind: "chat",
          sessionId: "session-a",
          title: "Alpha",
        },
      ],
      groups: [{ id: "main", tabIds: ["chat-1"], activeTabId: "chat-1" }],
      splitRoot: { type: "group", id: "main" },
      activeGroupId: "main",
      zoomedGroupId: null,
    };
    localStorage.setItem("medousa-home-shell-tabs-v2", JSON.stringify(v2));

    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    expect(shellTabs.desktops).toHaveLength(1);
    expect(shellTabs.activeDesktopName).toBe("Main");
    expect(shellTabs.activeTab?.kind).toBe("chat");
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toBeTruthy();
  });

  it("migrates durable Code descriptors from the v3 shell snapshot", async () => {
    const layout = {
      tabs: [{
        id: "shell-code",
        kind: "lme",
        lmeTabId: "code-file:work-a:src%2Flib.rs",
        title: "lib.rs",
      }],
      groups: [{ id: "main", tabIds: ["shell-code"], activeTabId: "shell-code" }],
      splitRoot: { type: "group", id: "main" },
      activeGroupId: "main",
      zoomedGroupId: null,
    };
    localStorage.setItem("medousa-home-shell-tabs-v3", JSON.stringify({
      desktops: [{ id: "desktop-main", name: "Main", layout }],
      activeDesktopId: "desktop-main",
    }));

    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    expect(lmeState.tabs).toEqual([expect.objectContaining({
      tabId: "code-file:work-a:src%2Flib.rs",
      kind: "code",
      workId: "work-a",
      resource: { kind: "file", path: "src/lib.rs", line: null },
    })]);
    expect(shellTabs.activeTab).toMatchObject({
      kind: "lme",
      lmeTabId: "code-file:work-a:src%2Flib.rs",
    });
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toBeTruthy();
  });

  it("switches desktops and keeps layouts independent", async () => {
    const { shellTabs } = await loadShellTabs();
    const { chatStreamPool } = await import("$lib/chat/chatStreamPool.svelte");
    shellTabs.openChat("session-a", { activate: true });
    const researchId = shellTabs.createDesktop("Research");
    await vi.waitFor(() => expect(shellTabs.activeDesktopId).toBe(researchId));
    expect(shellTabs.tabs).toHaveLength(0);
    shellTabs.openChat("session-b", { activate: true });
    expect(shellTabs.activeTab?.kind).toBe("chat");
    if (shellTabs.activeTab?.kind === "chat") {
      expect(shellTabs.activeTab.sessionId).toBe("session-b");
    }

    const mainId = shellTabs.desktops.find((d) => d.name === "Main")!.id;
    await shellTabs.switchDesktop(mainId);
    expect(shellTabs.activeDesktopName).toBe("Main");
    expect(shellTabs.activeTab?.kind).toBe("chat");
    if (shellTabs.activeTab?.kind === "chat") {
      expect(shellTabs.activeTab.sessionId).toBe("session-a");
    }
    expect(chatStreamPool.release).toHaveBeenCalled();
    expect(chatStreamPool.acquire).toHaveBeenCalled();
  });

  it("does not seed a new desktop with the focused chat from another desktop", async () => {
    const { shellTabs } = await loadShellTabs();
    const { chat } = await import("$lib/stores/chat.svelte");
    shellTabs.openChat("session-a", { activate: true });
    const researchId = shellTabs.createDesktop("Research");
    await vi.waitFor(() => expect(shellTabs.activeDesktopId).toBe(researchId));

    expect(shellTabs.tabs).toHaveLength(0);
    expect(shellTabs.openSurface("chat", { activate: true })).toBeNull();
    expect(chat.newSession).toHaveBeenCalled();
    expect(shellTabs.tabs).toHaveLength(0);
  });

  it("does not mirror notes from another desktop when the global LME tab changes", async () => {
    const { shellTabs } = await loadShellTabs();
    lmeState.tabs = [
      { tabId: "note-a", kind: "note", path: "notes/a.md", title: "A" },
      { tabId: "note-b", kind: "note", path: "notes/b.md", title: "B" },
    ];
    lmeState.activeTabId = "note-a";
    shellTabs.openLme("note-a", { activate: true, title: "A" });
    shellTabs.openLme("note-b", { activate: true, title: "B" });

    const researchId = shellTabs.createDesktop("Research");
    await vi.waitFor(() => expect(shellTabs.activeDesktopId).toBe(researchId));
    lmeState.activeTabId = "note-a";
    shellTabs.syncFromLmeWorkspace();

    expect(shellTabs.tabs).toHaveLength(0);
    expect(
      shellTabs.desktops.find((desktop) => desktop.name === "Main")?.layout.tabs,
    ).toHaveLength(2);
  });

  it("allows the same chat session on two desktops without stealing", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.openChat("session-a", { activate: true });
    const researchId = shellTabs.createDesktop("Research");
    await vi.waitFor(() => expect(shellTabs.activeDesktopId).toBe(researchId));
    const opened = shellTabs.openChat("session-a", { activate: true });
    expect(opened).toBeTruthy();
    expect(shellTabs.tabs.filter((tab) => tab.kind === "chat")).toHaveLength(1);
  });

  it("does not reassign desktops on routine persist (avoids effect storms)", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    shellTabs.openChat("session-a", { activate: true });
    const before = shellTabs.desktops;
    shellTabs.syncTitlesFromStores();
    shellTabs.syncFromLmeWorkspace();
    shellTabs.patchTitle(shellTabs.tabs[0]!.id, "Alpha renamed");
    expect(shellTabs.desktops).toBe(before);
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toContain("Alpha renamed");
  });

  it("lists chat sessions for live restore with active pane first", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.openChat("session-a", { activate: true });
    shellTabs.splitActive("right");
    // Move split leaves the source pane empty; open session-b there and keep
    // session-a focused in the split pane.
    const otherGroup = shellTabs.groups.find((g) => g.id !== shellTabs.activeGroupId);
    expect(otherGroup).toBeTruthy();
    if (otherGroup) {
      shellTabs.openChat("session-b", { activate: true, groupId: otherGroup.id });
    }
    const ids = shellTabs.chatSessionIdsForLiveRestore();
    expect(ids[0]).toBe("session-b");
    expect(ids).toEqual(expect.arrayContaining(["session-a", "session-b"]));
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("closing the last chat tab leaves the pane empty (no library placeholder)", async () => {
    const { shellTabs } = await loadShellTabs();
    const tabId = shellTabs.openChat("session-a", { activate: true });
    expect(tabId).toBeTruthy();
    shellTabs.close(tabId!);
    expect(shellTabs.tabs).toHaveLength(0);
    expect(shellTabs.activeTab).toBeNull();
  });

  it("enterLmeFamily does not seed empty Workspace or Code surface tabs", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    shellTabs.openChat("session-a", { activate: true });

    expect(shellTabs.enterLmeFamily("library")).toBeNull();
    expect(shellTabs.enterLmeFamily("code")).toBeNull();
    expect(shellTabs.tabs.some((tab) => tab.kind === "surface")).toBe(false);
    expect(shellTabs.activeTab).toMatchObject({ kind: "chat", sessionId: "session-a" });
    expect(layoutState.focusDesktopSurface).toHaveBeenCalledWith("library");
    expect(layoutState.focusDesktopSurface).toHaveBeenCalledWith("code");
  });

  it("enterLmeFamily reactivates an open document in that family", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    const note = {
      tabId: "note-a",
      kind: "note",
      path: "notes/a.md",
      title: "Alpha",
    };
    lmeState.tabs = [note];
    lmeState.activeTabId = note.tabId;
    shellTabs.openLme(note.tabId, { activate: true, title: note.title });
    shellTabs.openChat("session-a", { activate: true });

    const restored = shellTabs.enterLmeFamily("library");
    expect(restored).toBeTruthy();
    expect(shellTabs.activeTab).toMatchObject({
      kind: "lme",
      lmeTabId: note.tabId,
    });
    expect(shellTabs.tabs.some((tab) =>
      tab.kind === "surface" && (tab.surfaceId === "library" || tab.surfaceId === "code")
    )).toBe(false);
  });

  it("openDestination for notes/code uses enterLmeFamily", async () => {
    const { shellTabs } = await loadShellTabs();
    shellTabs.bootstrap();
    shellTabs.openChat("session-a", { activate: true });
    shellTabs.openDestination("notes");
    shellTabs.openDestination("code");
    expect(shellTabs.tabs.some((tab) => tab.kind === "surface")).toBe(false);
    expect(shellTabs.activeTab).toMatchObject({ kind: "chat" });
  });

  it("opens multiple distinct chat tabs in the same group", async () => {
    const { shellTabs } = await loadShellTabs();
    const a = shellTabs.openChat("session-a", { activate: true });
    const b = shellTabs.openChat("session-b", { activate: true });
    expect(a).toBeTruthy();
    expect(b).toBeTruthy();
    expect(a).not.toBe(b);
    expect(shellTabs.orderedTabs.filter((tab) => tab.kind === "chat")).toHaveLength(2);
    expect(shellTabs.activeTab?.kind).toBe("chat");
    if (shellTabs.activeTab?.kind === "chat") {
      expect(shellTabs.activeTab.sessionId).toBe("session-b");
    }
  });

  it("moves a tab between panes", async () => {
    const { shellTabs } = await loadShellTabs();
    const a = shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.groups).toHaveLength(2);
    const sourceId = shellTabs.groups.find((g) => g.tabIds.includes(a!))?.id;
    const destId = shellTabs.groups.find((g) => g.id !== sourceId)?.id;
    expect(sourceId && destId).toBeTruthy();
    shellTabs.moveTab(a!, destId!);
    const dest = shellTabs.groups.find((g) => g.id === destId);
    const source = shellTabs.groups.find((g) => g.id === sourceId);
    expect(dest?.tabIds).toContain(a);
    expect(source?.tabIds).not.toContain(a);
    expect(shellTabs.activeGroupId).toBe(destId);
  });
});
