import { beforeEach, describe, expect, it, vi } from "vitest";

const layoutState = {
  desktopSurface: "chat" as string,
  focusDesktopSurface: vi.fn((surface: string) => {
    layoutState.desktopSurface = surface;
  }),
  setShellSidebarMode: vi.fn(),
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

  it("opens chat tabs uniquely and activates the latest", async () => {
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

  it("opens singleton surface tabs once", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    const first = shellTabs.openSurface("peers", { activate: true });
    const second = shellTabs.openSurface("peers", { activate: true });
    expect(first).toBe(second);
    expect(shellTabs.orderedTabs.filter((tab) => tab.kind === "surface")).toHaveLength(1);
  });

  it("keeps a single editor group shaped for future splits", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    shellTabs.openSurface("context", { activate: true });
    expect(shellTabs.groups).toHaveLength(1);
    expect(shellTabs.groups[0]?.id).toBe("main");
    expect(shellTabs.groups[0]?.tabIds.length).toBeGreaterThan(0);
  });

  it("closing one chat tab leaves the other open", async () => {
    const { shellTabs } = await import("./shellTabs.svelte");
    const a = shellTabs.openChat("session-a", { activate: true });
    const b = shellTabs.openChat("session-b", { activate: true });
    expect(a && b).toBeTruthy();
    shellTabs.close(a!);
    expect(shellTabs.orderedTabs).toHaveLength(1);
    if (shellTabs.orderedTabs[0]?.kind === "chat") {
      expect(shellTabs.orderedTabs[0].sessionId).toBe("session-b");
    }
  });
});
