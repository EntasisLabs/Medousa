import { beforeEach, describe, expect, it, vi } from "vitest";

const shellTabs = {
  splitActive: vi.fn(() => true),
  focusDirection: vi.fn(),
  zoomToggle: vi.fn(),
  closeActiveGroup: vi.fn(() => true),
  openChat: vi.fn(),
  openDestination: vi.fn(),
  nextTabInActiveGroup: vi.fn(),
  prevTabInActiveGroup: vi.fn(),
  flashTabs: vi.fn(),
  switchDesktopAt: vi.fn(async () => true),
  clearZoom: vi.fn(),
  activeTab: null as { kind: string; sessionId: string } | null,
  zoomedGroupId: null as string | null,
};

vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs }));
vi.mock("$lib/stores/layout.svelte", () => ({
  layout: { toggleShellSidebarExpanded: vi.fn() },
}));

describe("dispatchPrefixCommand", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    shellTabs.activeTab = null;
  });

  it("splits right on %", async () => {
    const { dispatchPrefixCommand } = await import("./shellPaneHotkeys");
    expect(dispatchPrefixCommand("%", "%")).toBe(true);
    expect(shellTabs.splitActive).toHaveBeenCalledWith("right");
  });

  it("splits down on quote", async () => {
    const { dispatchPrefixCommand } = await import("./shellPaneHotkeys");
    expect(dispatchPrefixCommand('"', '"')).toBe(true);
    expect(shellTabs.splitActive).toHaveBeenCalledWith("down");
  });

  it("focuses with hjkl", async () => {
    const { dispatchPrefixCommand } = await import("./shellPaneHotkeys");
    dispatchPrefixCommand("h", "h");
    expect(shellTabs.focusDirection).toHaveBeenCalledWith("left");
    dispatchPrefixCommand("l", "l");
    expect(shellTabs.focusDirection).toHaveBeenCalledWith("right");
  });

  it("opens cheat sheet on ?", async () => {
    const { dispatchPrefixCommand } = await import("./shellPaneHotkeys");
    const onCheatSheet = vi.fn();
    expect(dispatchPrefixCommand("?", "?", { onCheatSheet })).toBe(true);
    expect(onCheatSheet).toHaveBeenCalled();
  });

  it("switches virtual desktop on 1–4", async () => {
    const { dispatchPrefixCommand } = await import("./shellPaneHotkeys");
    expect(dispatchPrefixCommand("2", "2")).toBe(true);
    expect(shellTabs.switchDesktopAt).toHaveBeenCalledWith(1);
    expect(dispatchPrefixCommand("5", "5")).toBe(false);
    expect(shellTabs.switchDesktopAt).toHaveBeenCalledTimes(1);
  });
});
