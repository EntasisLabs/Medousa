import { describe, expect, it } from "vitest";
import {
  clampContentZoom,
  CONTENT_ZOOM_DEFAULT,
  CONTENT_ZOOM_MAX,
  CONTENT_ZOOM_MIN,
  contentZoomPercent,
} from "./contentZoom";

describe("contentZoom", () => {
  it("clamps and snaps to steps", () => {
    expect(clampContentZoom(1)).toBe(1);
    expect(clampContentZoom(0.5)).toBe(CONTENT_ZOOM_MIN);
    expect(clampContentZoom(2)).toBe(CONTENT_ZOOM_MAX);
    expect(clampContentZoom(1.04)).toBe(1);
    expect(clampContentZoom(1.06)).toBe(1.1);
  });

  it("formats percent", () => {
    expect(contentZoomPercent(CONTENT_ZOOM_DEFAULT)).toBe("100%");
    expect(contentZoomPercent(1.2)).toBe("120%");
  });
});
