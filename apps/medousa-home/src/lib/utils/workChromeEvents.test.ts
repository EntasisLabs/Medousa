import { beforeEach, describe, expect, it, vi } from "vitest";
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { isMobile: false, setAskSheetOpen: vi.fn() } }));
import { layout } from "$lib/runtime/layout.svelte";
import { taskOverlays } from "$lib/runtime/taskOverlayState.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import { openNewAsk } from "./workChromeEvents";
beforeEach(() => { layout.isMobile = false; taskOverlays.newAskOpen = false; taskOverlays.askHistorySessionId = "chat"; commandSpotlight.openActivity(); vi.clearAllMocks(); });
describe("background ask entry", () => {
  it("opens over the desktop context and closes Activity/history", () => {
    openNewAsk();
    expect(taskOverlays.newAskOpen).toBe(true); expect(taskOverlays.askHistorySessionId).toBeNull(); expect(commandSpotlight.open).toBe(false);
    expect(layout.setAskSheetOpen).not.toHaveBeenCalled();
  });
  it("uses the mobile ask sheet without changing tabs", () => {
    layout.isMobile = true; openNewAsk();
    expect(layout.setAskSheetOpen).toHaveBeenCalledWith(true); expect(taskOverlays.newAskOpen).toBe(false);
  });
});
