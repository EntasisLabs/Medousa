import { describe, expect, it } from "vitest";
import type { SurfaceDef } from "$lib/types/environment";
import {
  navTier,
  shellSidebarViewTitle,
  surfaceHasShellSidebarView,
} from "./navSurfaces";

function surface(id: string): SurfaceDef {
  return {
    id,
    label: id,
    icon: "circle",
    kind: "builtin",
    builtinId: id,
    layout: "single",
    slots: [],
    mobileTab: null,
  };
}

describe("navSurfaces shell sidebar views", () => {
  it("hides runtime and messaging from the life rail", () => {
    expect(navTier(surface("runtime"))).toBe("hidden");
    expect(navTier(surface("messaging"))).toBe("hidden");
  });

  it("marks list / HUD surfaces as having a sidebar view", () => {
    expect(surfaceHasShellSidebarView("peers")).toBe(true);
    expect(surfaceHasShellSidebarView("chat")).toBe(true);
    expect(surfaceHasShellSidebarView("messaging")).toBe(true);
    expect(surfaceHasShellSidebarView("library")).toBe(true);
    expect(surfaceHasShellSidebarView("code")).toBe(true);
    expect(surfaceHasShellSidebarView("map")).toBe(true);
    expect(surfaceHasShellSidebarView("settings")).toBe(true);
    expect(surfaceHasShellSidebarView("work")).toBe(true);
    expect(surfaceHasShellSidebarView("calendar")).toBe(true);
    expect(surfaceHasShellSidebarView("web")).toBe(true);
    expect(surfaceHasShellSidebarView("profiles")).toBe(true);
  });

  it("leaves non-list surfaces without a view list", () => {
    expect(surfaceHasShellSidebarView("runtime")).toBe(false);
  });

  it("titles view lists for the active surface", () => {
    expect(shellSidebarViewTitle("peers")).toBe("Peers");
    expect(shellSidebarViewTitle("chat")).toBe("Sessions");
    expect(shellSidebarViewTitle("messaging")).toBe("Channels");
    expect(shellSidebarViewTitle("library")).toBe("Library");
    expect(shellSidebarViewTitle("code")).toBe("Projects");
    expect(shellSidebarViewTitle("map")).toBe("Map");
    expect(shellSidebarViewTitle("settings")).toBe("Settings");
    expect(shellSidebarViewTitle("calendar")).toBe("Calendar");
    expect(shellSidebarViewTitle("work")).toBe("Work");
    expect(shellSidebarViewTitle("web")).toBe("Web");
    expect(shellSidebarViewTitle("profiles")).toBe("You");
  });
});
