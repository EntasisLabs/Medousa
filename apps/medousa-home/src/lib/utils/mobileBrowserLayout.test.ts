import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clampDesktopEmbedBoundsToViewport,
  measureDesktopBrowserEmbedBounds,
  scaleDesktopEmbedBoundsForZoom,
} from "./mobileBrowserLayout";

describe("desktop browser embed bounds", () => {
  afterEach(() => vi.unstubAllGlobals());

  it.each([null, "0.9", "1", "1.1", "1.6"])(
    "keeps the full host inside a viewport using the same zoom units (%s)",
    (savedZoom) => {
      vi.stubGlobal("localStorage", { getItem: () => savedZoom });
      vi.stubGlobal("window", { innerWidth: 1200, innerHeight: 800 });
      const host = {
        getBoundingClientRect: () => ({ left: 300, top: 80, width: 900, height: 700 }),
      } as HTMLElement;
      const zoom = savedZoom === null ? 1.1 : Number(savedZoom);
      const bounds = measureDesktopBrowserEmbedBounds(host, host, host);
      expect(bounds?.x).toBeCloseTo(300 * zoom);
      expect(bounds?.y).toBeCloseTo(80 * zoom);
      expect(bounds?.width).toBeCloseTo(900 * zoom);
      expect(bounds?.height).toBeCloseTo(700 * zoom);
    },
  );

  it("scales CSS rects by Tauri content zoom so zoom-out does not oversize the native child", () => {
    expect(
      scaleDesktopEmbedBoundsForZoom(
        { x: 40, y: 80, width: 1000, height: 600 },
        0.9,
      ),
    ).toEqual({ x: 36, y: 72, width: 900, height: 540 });
  });

  it("leaves bounds unchanged at 100% zoom", () => {
    const bounds = { x: 10, y: 20, width: 800, height: 500 };
    expect(scaleDesktopEmbedBoundsForZoom(bounds, 1)).toEqual(bounds);
  });

  it("clamps an overflowing host rect to the visible viewport", () => {
    expect(
      clampDesktopEmbedBoundsToViewport(
        { x: -4, y: 40, width: 1400, height: 900 },
        { width: 1200, height: 800 },
      ),
    ).toEqual({ x: 0, y: 40, width: 1200, height: 760 });
  });
});
