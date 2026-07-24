import { describe, expect, it, vi } from "vitest";
import {
  ensureRailPopoverOpen,
  getRailPopoverChrome,
  setRailPopoverChrome,
} from "./railPopoverChrome";

describe("railPopoverChrome", () => {
  it("no-ops when nothing is registered", async () => {
    setRailPopoverChrome(null);
    await expect(ensureRailPopoverOpen()).resolves.toBeUndefined();
  });

  it("skips ensureOpen when already open", async () => {
    const ensureOpen = vi.fn(async () => {});
    setRailPopoverChrome({
      ensureOpen,
      isOpen: () => true,
    });
    await ensureRailPopoverOpen();
    expect(ensureOpen).not.toHaveBeenCalled();
    expect(getRailPopoverChrome()?.isOpen()).toBe(true);
    setRailPopoverChrome(null);
  });

  it("calls ensureOpen when compact", async () => {
    const ensureOpen = vi.fn(async () => {});
    setRailPopoverChrome({
      ensureOpen,
      isOpen: () => false,
    });
    await ensureRailPopoverOpen();
    expect(ensureOpen).toHaveBeenCalledTimes(1);
    setRailPopoverChrome(null);
  });
});
