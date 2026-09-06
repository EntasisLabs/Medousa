import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  lifecycle: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock("$lib/daemon/browserWorlds", () => ({
  listIsolatedBrowserWorlds: (...args: unknown[]) => mocks.list(...args),
  setIsolatedBrowserWorldLifecycle: (...args: unknown[]) => mocks.lifecycle(...args),
  navigateIsolatedBrowserWorld: (...args: unknown[]) => mocks.navigate(...args),
  createIsolatedBrowserWorld: vi.fn(),
  observeIsolatedBrowserWorld: vi.fn(),
  screenshotIsolatedBrowserWorld: vi.fn(),
  inputIsolatedBrowserWorld: vi.fn(),
}));

vi.mock("$lib/utils/resolveBrowserDestination", () => ({
  resolveBrowserDestination: async (input: string) => input,
}));

import { GovernedBrowserStore } from "$lib/stores/governedBrowser.svelte";

function world(worldId: string) {
  return {
    world_id: worldId,
    owner_profile_id: "user:test",
    authority_id: "workshop:test",
    driver: {
      driver_id: `driver:${worldId}`,
      ownership: "owned" as const,
      display_name: worldId,
    },
    tab_group_id: `tabs:${worldId}`,
    tab_id: `tab:${worldId}`,
    profile: { kind: "ephemeral" as const },
    run_state: "running" as const,
    control: "agent" as const,
    control_epoch: 1,
    view_attached: true,
    headless: true,
    url: "about:blank",
    title: "",
    created_at_ms: 1,
    updated_at_ms: 1,
  };
}

describe("GovernedBrowserStore runtime binding", () => {
  beforeEach(() => {
    mocks.list.mockReset();
    mocks.lifecycle.mockReset();
    mocks.navigate.mockReset();
  });

  it("keeps an active world on its owning runtime when the creation target changes", async () => {
    const local = world("world:local");
    const remote = world("world:remote");
    mocks.list.mockResolvedValueOnce([local]).mockResolvedValueOnce([remote]);
    mocks.lifecycle.mockResolvedValue(local);
    mocks.navigate.mockResolvedValue({ ...local, url: "https://example.com" });
    const store = new GovernedBrowserStore();

    await store.load("scope-test", {
      runtimeId: "runtime-local",
      parentRuntimeId: "runtime-local",
      runtimeLabels: {
        "runtime-local": "This Mac",
        "runtime-remote": "Mac mini",
      },
    });
    await store.selectWorld(local.world_id, "runtime-local");
    await store.selectTarget({
      runtimeId: "runtime-remote",
      parentRuntimeId: "runtime-local",
      runtimeLabels: {
        "runtime-local": "This Mac",
        "runtime-remote": "Mac mini",
      },
    });

    expect(store.targetRuntimeId).toBe("runtime-remote");
    expect(store.selectedWorld?.world_id).toBe(local.world_id);
    await store.navigate("https://example.com");
    expect(mocks.navigate).toHaveBeenCalledWith(
      local.world_id,
      "https://example.com",
      null,
    );
  });
});
