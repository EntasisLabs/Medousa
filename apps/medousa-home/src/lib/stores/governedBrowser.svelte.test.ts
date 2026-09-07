import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  lifecycle: vi.fn(),
  navigate: vi.fn(),
  observe: vi.fn(),
  screenshot: vi.fn(),
  presentation: vi.fn(),
}));

vi.mock("$lib/daemon/browserWorlds", () => ({
  listIsolatedBrowserWorlds: (...args: unknown[]) => mocks.list(...args),
  setIsolatedBrowserWorldLifecycle: (...args: unknown[]) => mocks.lifecycle(...args),
  navigateIsolatedBrowserWorld: (...args: unknown[]) => mocks.navigate(...args),
  openIsolatedBrowserPresentation: (...args: unknown[]) => mocks.presentation(...args),
  createIsolatedBrowserWorld: vi.fn(),
  observeIsolatedBrowserWorld: (...args: unknown[]) => mocks.observe(...args),
  screenshotIsolatedBrowserWorld: (...args: unknown[]) => mocks.screenshot(...args),
  inputIsolatedBrowserWorld: vi.fn(),
}));

vi.mock("$lib/utils/resolveBrowserDestination", () => ({
  resolveBrowserDestination: async (input: string) => input,
}));

import { GovernedBrowserStore } from "$lib/stores/governedBrowser.svelte";
import type { BrowserPresentationStreamOptions } from "$lib/daemon/browserWorlds";
import type { BrowserPresentationFrame } from "$lib/types/generated/daemon_api";

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
    mocks.observe.mockReset();
    mocks.screenshot.mockReset();
    mocks.presentation.mockReset();
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
    expect(store.turnWorldSelection).toEqual({
      world_id: local.world_id,
      execution_runtime_id: "runtime-local",
    });
    await store.navigate("https://example.com");
    expect(mocks.navigate).toHaveBeenCalledWith(
      local.world_id,
      "https://example.com",
      null,
    );
  });

  it("does not advertise a stale world that was not loaded from its workshop", () => {
    const store = new GovernedBrowserStore();
    store.source = {
      kind: "workshop",
      worldId: "world:stale",
      runtimeId: "runtime-offline",
    };

    expect(store.selectedWorld).toBeNull();
    expect(store.turnWorldSelection).toBeNull();
  });

  it("renders one atomically paired destination stream instead of polling two endpoints", async () => {
    const selected = world("world:remote");
    const close = vi.fn();
    let options: BrowserPresentationStreamOptions | undefined;
    mocks.list.mockResolvedValue([selected]);
    mocks.lifecycle.mockResolvedValue(selected);
    mocks.presentation.mockImplementation(async (next: BrowserPresentationStreamOptions) => {
      options = next;
      next.onOpen?.();
      return { close, closed: false };
    });
    const store = new GovernedBrowserStore();
    await store.load("scope-test", {
      runtimeId: "runtime-remote",
      parentRuntimeId: "runtime-local",
      runtimeLabels: { "runtime-remote": "Mac mini" },
    });
    await store.selectWorld(selected.world_id, "runtime-remote");

    const stop = store.startPresentation(selected.world_id, () => 900);
    await vi.waitFor(() => expect(mocks.presentation).toHaveBeenCalledOnce());

    const viewport = {
      width: 1280,
      height: 900,
      scroll_x: 0,
      scroll_y: 0,
      device_scale_factor: 1,
    };
    const frame: BrowserPresentationFrame = {
      schema_version: 1,
      world_id: selected.world_id,
      observation: {
        schema_version: 1,
        tab_id: selected.tab_id!,
        url: "https://example.com/",
        title: "Example",
        document_id: "doc-1",
        revision: 4,
        full: false,
        viewport,
        truncated: false,
        captured_at_ms: 10,
        untrusted_content: true,
      },
      screenshot: {
        schema_version: 1,
        tab_id: selected.tab_id!,
        url: "https://example.com/",
        title: "Example",
        document_id: "doc-1",
        observation_revision: 4,
        viewport,
        coordinate_frame: "css_viewport",
        mime: "image/jpeg",
        image_width: 900,
        image_height: 633,
        byte_size: 1234,
        sha256: "abc",
        sensitive_regions_redacted: 0,
        captured_at_ms: 11,
        untrusted_content: true,
        image_base64: "frame",
      },
    };
    options?.onFrame(frame);

    expect(store.observation?.revision).toBe(4);
    expect(store.screenshot?.image_base64).toBe("frame");
    expect(store.activeTitle).toBe("Example");
    expect(mocks.observe).not.toHaveBeenCalled();
    expect(mocks.screenshot).not.toHaveBeenCalled();
    stop();
    expect(close).toHaveBeenCalledOnce();
  });
});
