/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import NavShell from "./NavShell.svelte";
import { layout } from "$lib/runtime/layout.svelte";
import { environment } from "$lib/stores/environment.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import type { SurfaceDef } from "$lib/types/environment";

let component: ReturnType<typeof mount> | undefined;
const onSelect = vi.fn();
const ids = ["chat", "code", "reviews", "notes", "files", "artifacts", "workshop", "map", "peers", "web", "calendar"];
const surfaces: SurfaceDef[] = [...ids, "automations"].map(id => ({
  id, label: id, icon: "file", kind: "builtin", builtinId: id,
  layout: "single", slots: [], mobileTab: null,
}));
surfaces.push({ id: "custom-view", label: "Custom view", icon: "file", kind: "custom", layout: "single", slots: [] });

function clickDestination(id: string) {
  const button = document.querySelector<HTMLButtonElement>(`[data-rail-surface="${id}"]`);
  expect(button).not.toBeNull();
  button!.click();
  flushSync();
}

beforeEach(() => {
  localStorage.clear();
  layout.setShellSidebarMode("nav");
  layout.stopRailLayoutEditing();
  environment.spec = defaultEnvironmentSpec();
  vi.spyOn(environment, "navSurfaces").mockReturnValue(surfaces);
  // Isolate the row's routing decision from the list's own network/lifecycle.
  vi.spyOn(layout, "openShellSidebarView").mockImplementation(() => {});
  vi.spyOn(lmeWorkspace, "setExplorerMode").mockImplementation(() => {});
  component = mount(NavShell, { target: document.body, props: {
    active: "reviews", onSelect, onOpenChat: vi.fn(),
  } });
  flushSync();
});

afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
  environment.spec = null;
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

it.each([...ids, "profiles", "settings"])("browses %s without requesting content-tab navigation", (id) => {
  clickDestination(id);
  expect(layout.openShellSidebarView).toHaveBeenCalledExactlyOnceWith(id);
  expect(onSelect).not.toHaveBeenCalled();
});

it("sets the explorer family without selecting a remembered document", () => {
  clickDestination("code");
  expect(lmeWorkspace.setExplorerMode).toHaveBeenLastCalledWith("code");
  clickDestination("files");
  expect(lmeWorkspace.setExplorerMode).toHaveBeenLastCalledWith("files");
  clickDestination("workshop");
  expect(lmeWorkspace.setExplorerMode).toHaveBeenLastCalledWith("agents");
  expect(onSelect).not.toHaveBeenCalled();
});

it("still opens Automations directly because its run list occupies the main pane", () => {
  clickDestination("automations");
  expect(onSelect).toHaveBeenCalledExactlyOnceWith("automations");
  expect(layout.openShellSidebarView).not.toHaveBeenCalled();
});

it("still opens a custom destination with no side-rail list", () => {
  clickDestination("custom-view");
  expect(onSelect).toHaveBeenCalledExactlyOnceWith("custom-view");
  expect(layout.openShellSidebarView).not.toHaveBeenCalled();
});
