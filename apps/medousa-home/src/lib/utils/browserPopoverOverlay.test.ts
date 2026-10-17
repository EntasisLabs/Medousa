import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/platform", () => ({ isTauri: () => true }));

describe("native browser overlays", () => {
  beforeEach(() => { vi.useFakeTimers(); vi.resetModules(); });
  afterEach(() => { vi.useRealTimers(); });

  it("keeps native content covered until the last nested menu closes", async () => {
    const overlay = await import("./browserPopoverOverlay");
    const depths: number[] = [];
    const restored: number[] = [];
    overlay.setBrowserPopoverLayoutPort({
      flushLayout: async () => { depths.push(overlay.getBrowserPopoverOverlayDepth()); },
      scheduleLayout: () => { restored.push(overlay.getBrowserPopoverOverlayDepth()); },
    });

    await overlay.pushBrowserPopoverOverlay();
    await overlay.pushBrowserPopoverOverlay();
    await overlay.popBrowserPopoverOverlay();
    expect(depths).toEqual([1, 2, 1]);
    expect(restored).toEqual([]);
    await overlay.popBrowserPopoverOverlay();
    await vi.runAllTimersAsync();
    expect(restored).toEqual([0]);
  });

  it("cancels a pending page restore when another menu opens", async () => {
    const overlay = await import("./browserPopoverOverlay");
    const restore = vi.fn();
    overlay.setBrowserPopoverLayoutPort({ flushLayout: async () => {}, scheduleLayout: restore });
    await overlay.pushBrowserPopoverOverlay();
    await overlay.popBrowserPopoverOverlay();
    await overlay.pushBrowserPopoverOverlay();
    await vi.runAllTimersAsync();
    expect(restore).not.toHaveBeenCalled();
    await overlay.popBrowserPopoverOverlay();
    await vi.runAllTimersAsync();
    expect(restore).toHaveBeenCalledOnce();
  });

  it("does not restore a window after its layout owner unmounts", async () => {
    const overlay = await import("./browserPopoverOverlay");
    const restore = vi.fn();
    overlay.setBrowserPopoverLayoutPort({ flushLayout: async () => {}, scheduleLayout: restore });
    await overlay.pushBrowserPopoverOverlay();
    await overlay.popBrowserPopoverOverlay();
    overlay.setBrowserPopoverLayoutPort(null);
    await vi.runAllTimersAsync();
    expect(restore).not.toHaveBeenCalled();
  });
});
