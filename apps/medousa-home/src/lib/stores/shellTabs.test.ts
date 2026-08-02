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
    switchSession: vi.fn(async function (this: { sessionId: string }, id: string) {
      this.sessionId = id;
    }),
    newSession: vi.fn(async () => {}),
  },
}));

vi.mock("$lib/stores/chatStreamPool.svelte", () => ({
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

vi.mock("$lib/stores/layout.svelte", () => ({
  layout: layoutState,
}));

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
    const { shellTabs } = await import("./shellTabs.svelte");

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
    const { shellTabs } = await import("./shellTabs.svelte");
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

  it("keeps governed terminal ownership on the shell tab", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.groups).toHaveLength(2);
  });

  it("splits by moving the active tab into the new pane", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    expect(shellTabs.activeTab?.id).toBe(shellId);
    expect(shellTabs.tabs.filter((tab) => tab.kind === "lme")).toHaveLength(1);
    const from = shellTabs.groups.find((group) => group.id === fromGroupId);
    const to = shellTabs.groups.find((group) => group.id === shellTabs.activeGroupId);
    expect(from?.tabIds).not.toContain(shellId);
    expect(to?.tabIds).toContain(shellId);
  });

  it("splits a host pane with a dragged tab on an edge", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(true);
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(false);
    expect(shellTabs.paneCount).toBe(4);
  });

  it("moves a tab to another desktop", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    const stayId = shellTabs.openChat("session-a", { activate: true });
    const moveId = shellTabs.openChat("session-b", { activate: true });
    expect(stayId).toBeTruthy();
    expect(moveId).toBeTruthy();
    // splitActive moves the focused tab into the new pane
    expect(shellTabs.splitActive("right")).toBe(true);
    const rightGroup = shellTabs.activeGroupId;
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.activeTab?.id).toBe(moveId);

    const otherId = shellTabs.createDesktop("Park", { activate: false });
    expect(shellTabs.movePaneToDesktop(rightGroup, otherId)).toBe(true);
    expect(shellTabs.paneCount).toBe(1);
    expect(shellTabs.tabs.some((tab) => tab.id === moveId)).toBe(false);
    expect(shellTabs.tabs.some((tab) => tab.id === stayId)).toBe(true);
    const park = shellTabs.desktops.find((desktop) => desktop.id === otherId);
    expect(park?.layout.tabs.some((tab) => tab.id === moveId)).toBe(true);
  });

  it("collects search hits across desktops and reveals them", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    const tabId = shellTabs.openChat("session-a", { activate: true });
    expect(tabId).toBeTruthy();
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.paneCount).toBe(2);
    expect(shellTabs.activeTab?.id).toBe(tabId);
    expect(shellTabs.closeActiveGroup()).toBe(true);
    expect(shellTabs.paneCount).toBe(1);
    expect(shellTabs.tabs.some((tab) => tab.id === tabId)).toBe(true);
    expect(shellTabs.activeTab?.id).toBe(tabId);
    expect(shellTabs.closeActiveGroup()).toBe(false);
  });

  it("opens singleton surface tabs once", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    const first = shellTabs.openSurface("peers", { activate: true });
    const second = shellTabs.openSurface("peers", { activate: true });
    expect(first).toBe(second);
    expect(shellTabs.orderedTabs.filter((tab) => tab.kind === "surface")).toHaveLength(1);
  });

  it("keeps editor groups shaped for splits", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openSurface("map", { activate: true });
    expect(shellTabs.groups.length).toBeGreaterThanOrEqual(1);
    expect(shellTabs.splitRoot.type).toBe("group");
  });

  it("persists and restores split layout across bootstrap", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs: restored } = await import("./shellTabs.svelte");
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

  it("isolates durable workspace sessions by workshop", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
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

    vi.resetModules();
    lmeState.tabs = [];
    lmeState.activeTabId = null;
    const { shellTabs: restored } = await import("./shellTabs.svelte");
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

    const { shellTabs } = await import("./shellTabs.svelte");
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

    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    const { chatStreamPool } = await import("./chatStreamPool.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    const researchId = shellTabs.createDesktop("Research");
    await vi.waitFor(() => expect(shellTabs.activeDesktopId).toBe(researchId));
    const opened = shellTabs.openChat("session-a", { activate: true });
    expect(opened).toBeTruthy();
    expect(shellTabs.tabs.filter((tab) => tab.kind === "chat")).toHaveLength(1);
  });

  it("does not reassign desktops on routine persist (avoids effect storms)", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    const before = shellTabs.desktops;
    shellTabs.syncTitlesFromStores();
    shellTabs.syncFromLmeWorkspace();
    shellTabs.patchTitle(shellTabs.tabs[0]!.id, "Alpha renamed");
    expect(shellTabs.desktops).toBe(before);
    expect(localStorage.getItem("medousa-home-workspace-session-v4:personal")).toContain("Alpha renamed");
  });

  it("lists chat sessions for live restore with active pane first", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    shellTabs.splitActive("right");
    // Split moves session-a into the new pane; open distinct chat in the empty pane.
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
    const { shellTabs } = await import("./shellTabs.svelte");
    const tabId = shellTabs.openChat("session-a", { activate: true });
    expect(tabId).toBeTruthy();
    shellTabs.close(tabId!);
    expect(shellTabs.tabs).toHaveLength(0);
    expect(shellTabs.activeTab).toBeNull();
  });

  it("opens multiple distinct chat tabs in the same group", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
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
    const { shellTabs } = await import("./shellTabs.svelte");
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
