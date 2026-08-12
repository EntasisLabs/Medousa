import { describe, expect, it } from "vitest";
import {
  clampDesktopEmbedBoundsToViewport,
  scaleDesktopEmbedBoundsForZoom,
} from "./mobileBrowserLayout";

describe("desktop browser embed bounds", () => {
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
