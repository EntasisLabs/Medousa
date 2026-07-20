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

vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({
  lmeWorkspace: {
    tabs: [],
    activeTabId: null,
    activeTab: null,
    activateTab: vi.fn(async () => {}),
    closeTab: vi.fn(async () => {}),
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
});
