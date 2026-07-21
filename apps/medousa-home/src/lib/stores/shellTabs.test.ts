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
    switchSession: vi.fn(async function (this: { sessionId: string }, id: string) {
      this.sessionId = id;
    }),
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
  tabs: [] as Array<{
    tabId: string;
    kind: string;
    path?: string;
    title: string;
  }>,
  activeTabId: null as string | null,
  get activeTab() {
    return this.tabs.find((tab) => tab.tabId === this.activeTabId) ?? null;
  },
  activateTab: vi.fn(async () => {}),
  closeTab: vi.fn(async () => {}),
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

  it("refuses a fifth pane", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(true);
    expect(shellTabs.splitActive("right")).toBe(true);
    expect(shellTabs.splitActive("down")).toBe(false);
    expect(shellTabs.paneCount).toBe(4);
  });

  it("closes a pane and keeps at least one", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openChat("session-a", { activate: true });
    shellTabs.splitActive("right");
    expect(shellTabs.closeActiveGroup()).toBe(true);
    expect(shellTabs.paneCount).toBe(1);
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
    shellTabs.openSurface("context", { activate: true });
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

    vi.resetModules();
    const { shellTabs: restored } = await import("./shellTabs.svelte");
    restored.bootstrap();
    expect(restored.paneCount).toBe(2);
    expect(restored.activeGroupId).toBe(activeGroup);
    expect(restored.zoomedGroupId).toBe(zoomed);
    expect(restored.splitRoot.type).toBe("branch");
    if (restored.splitRoot.type === "branch") {
      expect(restored.splitRoot.ratio).toBeCloseTo(0.35);
    }
    expect(restored.chatSessionIdsForLiveRestore()).toContain("session-a");
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
