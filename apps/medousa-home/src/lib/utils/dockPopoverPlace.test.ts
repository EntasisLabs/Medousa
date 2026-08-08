import { beforeEach, describe, expect, it, vi } from "vitest";
import { placeDockPopover } from "./dockPopoverPlace";

function fakeTrigger(rect: {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width?: number;
  height?: number;
}): HTMLElement {
  const width = rect.width ?? rect.right - rect.left;
  const height = rect.height ?? rect.bottom - rect.top;
  return {
    getBoundingClientRect: () =>
      ({
        x: rect.left,
        y: rect.top,
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        width,
        height,
        toJSON() {
          return {};
        },
      }) as DOMRect,
  } as HTMLElement;
}

describe("placeDockPopover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", {
      innerWidth: 1200,
      innerHeight: 800,
    });
  });

  it("preferUp false opens below when the trigger is near the top", () => {
    const trigger = fakeTrigger({ left: 40, top: 48, right: 120, bottom: 72 });

    const place = placeDockPopover(trigger, {
      preferUp: false,
      width: 196,
      maxHeight: 320,
    });

    expect(place.transform).toBe("none");
    expect(place.top).toBe(72 + 6);
    expect(place.maxHeight).toBeGreaterThan(160);
  });

  it("preferUp false flips up when below is too short", () => {
    const trigger = fakeTrigger({ left: 40, top: 700, right: 120, bottom: 724 });

    const place = placeDockPopover(trigger, {
      preferUp: false,
      width: 196,
      maxHeight: 320,
    });

    expect(place.transform).toBe("translateY(-100%)");
    expect(place.top).toBe(700 - 6);
    expect(place.maxHeight).toBeLessThanOrEqual(700 - 8);
  });

  it("never forces maxHeight taller than available space", () => {
    // Equal tiny pockets — prefer-up opens above into 50px and must not invent a 120px floor.
    const trigger = fakeTrigger({ left: 40, top: 58, right: 120, bottom: 742 });

    const place = placeDockPopover(trigger, {
      preferUp: true,
      width: 196,
      maxHeight: 320,
    });

    expect(place.transform).toBe("translateY(-100%)");
    expect(place.maxHeight).toBe(50);
  });
});
