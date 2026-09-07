import { describe, expect, it } from "vitest";
import type { IsolatedBrowserWorld } from "$lib/daemon/browserWorlds";
import {
  chooseBrowserSurfaceSource,
  pointInCssViewport,
} from "$lib/browser/worldPresentation";

function world(
  worldId: string,
  updatedAt: number,
  options?: { attached?: boolean; state?: IsolatedBrowserWorld["run_state"] },
): IsolatedBrowserWorld {
  return {
    world_id: worldId,
    owner_profile_id: "user:test",
    authority_id: "workshop:test",
    driver: {
      driver_id: `driver:${worldId}`,
      ownership: "owned",
      display_name: worldId,
    },
    tab_group_id: `tabs:${worldId}`,
    tab_id: `tab:${worldId}`,
    profile: { kind: "ephemeral" },
    run_state: options?.state ?? "running",
    control: "agent",
    control_epoch: 1,
    view_attached: options?.attached ?? true,
    headless: true,
    url: "about:blank",
    title: "",
    created_at_ms: updatedAt,
    updated_at_ms: updatedAt,
  };
}

describe("chooseBrowserSurfaceSource", () => {
  it("honors an explicit existing world", () => {
    expect(
      chooseBrowserSurfaceSource([world("one", 10), world("two", 20)], {
        source: { kind: "workshop", worldId: "one", runtimeId: "runtime-one" },
        selectedAt: 30,
      }, "runtime-one"),
    ).toEqual({ kind: "workshop", worldId: "one", runtimeId: "runtime-one" });
  });

  it("surfaces a newly attached agent world after a device choice", () => {
    expect(
      chooseBrowserSurfaceSource([world("agent", 40)], {
        source: { kind: "device" },
        selectedAt: 30,
      }, "runtime-two"),
    ).toEqual({ kind: "workshop", worldId: "agent", runtimeId: "runtime-two" });
  });

  it("never silently substitutes an old or failed world for device browsing", () => {
    expect(
      chooseBrowserSurfaceSource([world("old", 10), world("failed", 50, { state: "failed" })], {
        source: { kind: "device" },
        selectedAt: 30,
      }, "runtime-three"),
    ).toEqual({ kind: "device" });
  });
});

describe("pointInCssViewport", () => {
  const viewport = {
    width: 1280,
    height: 720,
    scroll_x: 0,
    scroll_y: 0,
    device_scale_factor: 1,
  };

  it("maps a contained screenshot without counting letterboxing", () => {
    expect(
      pointInCssViewport(500, 400, { left: 0, top: 0, width: 1000, height: 800 }, viewport),
    ).toEqual({ x: 640, y: 360 });
    expect(
      pointInCssViewport(500, 40, { left: 0, top: 0, width: 1000, height: 800 }, viewport),
    ).toBeNull();
  });
});
