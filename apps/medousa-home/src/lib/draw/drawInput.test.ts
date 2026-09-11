import { describe, expect, it } from "vitest";
import {
  coalescedPointerSamples,
  drawInputKind,
  drawPointFromPointer,
  expressivePenPressure,
  shouldDrawWithPointer,
} from "./drawInput";

describe("draw input", () => {
  it("normalizes pen pressure, timing, and tilt", () => {
    const point = drawPointFromPointer(
      { clientX: 10, clientY: 20, pressure: 0.7345, pointerType: "pen", timeStamp: 112, tiltX: 15, tiltY: -12 },
      (x, y) => ({ x: x * 2, y: y * 2 }),
      100,
    );
    expect(point).toEqual({ x: 20, y: 40, pressure: 1, elapsedMs: 12, tiltX: 15, tiltY: -12 });
  });

  it("expands ordinary Apple Pencil pressure into a visible expressive range", () => {
    expect(expressivePenPressure(0.15)).toBeLessThan(0.05);
    expect(expressivePenPressure(0.5)).toBeGreaterThan(0.7);
    expect(expressivePenPressure(0.7)).toBe(1);
  });

  it("leaves fake mouse pressure for velocity fallback", () => {
    const point = drawPointFromPointer(
      { clientX: 1, clientY: 2, pressure: 0.5, pointerType: "mouse", timeStamp: 5, tiltX: 0, tiltY: 0 },
      (x, y) => ({ x, y }),
      0,
    );
    expect(point?.pressure).toBeUndefined();
  });

  it("arbitrates pen, touch, and unknown inputs", () => {
    expect(drawInputKind("pen")).toBe("pen");
    expect(drawInputKind("banana")).toBe("unknown");
    expect(shouldDrawWithPointer("touch", false, false)).toBe(false);
    expect(shouldDrawWithPointer("touch", true, true)).toBe(false);
    expect(shouldDrawWithPointer("pen", false, false)).toBe(true);
  });

  it("keeps coalesced samples and appends the dispatched sample once", () => {
    const first = { clientX: 1, clientY: 1, timeStamp: 1 } as PointerEvent;
    const current = {
      clientX: 3,
      clientY: 3,
      timeStamp: 3,
      getCoalescedEvents: () => [first],
    } as unknown as PointerEvent;
    expect(coalescedPointerSamples(current)).toEqual([first, current]);

    const alreadyIncluded = {
      ...current,
      getCoalescedEvents: () => [first, current],
    } as unknown as PointerEvent;
    expect(coalescedPointerSamples(alreadyIncluded)).toEqual([first, current]);
  });
});
