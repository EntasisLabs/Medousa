/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  applyContentZoomCss,
  clampContentZoom,
  CONTENT_ZOOM_DEFAULT,
  CONTENT_ZOOM_MAX,
  CONTENT_ZOOM_MIN,
  contentZoomPercent,
} from "./contentZoom";

vi.mock("$lib/platform", () => ({
  isTauri: () => false,
}));

describe("contentZoom", () => {
  afterEach(() => {
    const root = document.documentElement;
    root.style.zoom = "";
    root.style.width = "";
    root.style.height = "";
    root.style.setProperty("--content-zoom", "1");
    root.style.removeProperty("--ui-zoom");
  });

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

  it("clears CSS zoom hacks when applying", () => {
    const root = document.documentElement;
    root.style.zoom = "1.2";
    root.style.width = "83%";
    root.style.height = "83%";
    root.style.setProperty("--ui-zoom", "1.2");

    applyContentZoomCss(1.2);
    expect(root.style.zoom).toBe("");
    expect(root.style.width).toBe("");
    expect(root.style.height).toBe("");
    expect(root.style.getPropertyValue("--content-zoom")).toBe("1");
    expect(root.style.getPropertyValue("--ui-zoom")).toBe("");
  });
});
