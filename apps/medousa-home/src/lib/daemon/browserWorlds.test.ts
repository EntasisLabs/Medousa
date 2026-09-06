import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  unary: vi.fn(),
}));

vi.mock("$lib/daemon/contractClient", () => ({
  daemonUnary: (...args: unknown[]) => mocks.unary(...args),
}));

import {
  createIsolatedBrowserWorld,
  listIsolatedBrowserWorlds,
  navigateIsolatedBrowserWorld,
} from "$lib/daemon/browserWorlds";

describe("isolated browser world client", () => {
  beforeEach(() => {
    mocks.unary.mockReset();
  });

  it("carries the exact owning runtime across list, create, and mutation", async () => {
    const world = { world_id: "world:browser:test" };
    mocks.unary
      .mockResolvedValueOnce({ ok: true, worlds: [world] })
      .mockResolvedValueOnce({ ok: true, world })
      .mockResolvedValueOnce({ ok: true, world });

    await listIsolatedBrowserWorlds("runtime-mac-mini");
    await createIsolatedBrowserWorld(undefined, "runtime-mac-mini");
    await navigateIsolatedBrowserWorld(
      world.world_id,
      "https://example.com",
      "runtime-mac-mini",
    );

    expect(mocks.unary.mock.calls).toEqual([
      ["browser.worlds.isolated.get", {}, undefined, "runtime-mac-mini"],
      [
        "browser.worlds.isolated.post",
        {},
        {
          display_name: "Workshop browser",
          profile: { kind: "ephemeral" },
          initial_url: "about:blank",
          headless: true,
        },
        "runtime-mac-mini",
      ],
      [
        "browser.worlds.isolated.by_world_id.navigate.post",
        { world_id: world.world_id },
        { url: "https://example.com" },
        "runtime-mac-mini",
      ],
    ]);
  });
});
