import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  unary: vi.fn(),
}));

vi.mock("$lib/daemon/contractClient", () => ({
  daemonUnary: (...args: unknown[]) => mocks.unary(...args),
}));

import {
  controlComputerDriver,
  listComputerDrivers,
  loadComputerDriverReadiness,
  preflightComputerDriver,
  watchComputerDriver,
  type ComputerDriverRegistration,
  type ComputerWorldControlState,
} from "$lib/daemon/computer";

const driver: ComputerDriverRegistration = {
  driver_id: "driver:computer:macos:accessibility",
  kind: "native_desktop",
  surface: "desktop",
  ownership: "attached",
  transport: "local_sidecar",
  capabilities: [
    "semantic_observation",
    "pixel_observation",
    "interaction",
    "human_takeover",
  ],
  display_name: "macOS Accessibility",
};

const control: ComputerWorldControlState = {
  world_id: "world:computer:one",
  driver_id: driver.driver_id,
  resource_id: "desktop:one",
  session_id: "session:one",
  ownership: "attached",
  holder: "available",
  requester_has_control: false,
  control_generation: 0,
  revision: 0,
};

describe("computer driver client", () => {
  beforeEach(() => {
    mocks.unary.mockReset();
  });

  it("uses the generated driver inventory and preflight operations", async () => {
    mocks.unary
      .mockResolvedValueOnce({ ok: true, drivers: [driver] })
      .mockResolvedValueOnce({
        ok: true,
        resource_id: "desktop:one",
        preflight: {
          protocol_version: 5,
          driver_id: driver.driver_id,
          platform: "macos",
          session_id: "session:one",
          permissions: [],
          checked_at_ms: 1,
        },
        control,
      });

    expect(await listComputerDrivers()).toEqual([driver]);
    expect(await preflightComputerDriver(driver.driver_id)).toMatchObject({
      resourceId: "desktop:one",
      preflight: { session_id: "session:one" },
      control: { holder: "available" },
    });
    expect(mocks.unary.mock.calls).toEqual([
      ["computer.drivers.get"],
      ["computer.drivers.by_driver_id.preflight.get", { driver_id: driver.driver_id }],
    ]);
  });

  it("keeps registered drivers visible when one preflight fails", async () => {
    const second = { ...driver, driver_id: "driver:computer:second" };
    mocks.unary.mockImplementation(async (operation: string, params?: Record<string, string>) => {
      if (operation === "computer.drivers.get") return { ok: true, drivers: [driver, second] };
      if (params?.driver_id === driver.driver_id) {
        return {
          ok: true,
          resource_id: "desktop:one",
          preflight: {
            protocol_version: 5,
            driver_id: driver.driver_id,
            platform: "macos",
            session_id: "session:one",
            permissions: [],
            checked_at_ms: 1,
          },
          control,
        };
      }
      throw new Error("sidecar unavailable");
    });

    const readiness = await loadComputerDriverReadiness();
    expect(readiness).toHaveLength(2);
    expect(readiness[0]).toMatchObject({ resourceId: "desktop:one" });
    expect(readiness[1]).toMatchObject({
      driver: { driver_id: second.driver_id },
      error: "sidecar unavailable",
    });
  });

  it("requests one bounded frame and changes daemon-owned control", async () => {
    const frame = {
      observation: {
        generation: "generation:one",
        revision: 2,
        focused_window_resource_id: "window:one",
      },
      capture: {
        mime: "image/png",
        image_base64: "eA==",
      },
      control,
    };
    mocks.unary.mockResolvedValueOnce(frame).mockResolvedValueOnce({
      ok: true,
      control: { ...control, holder: "human", requester_has_control: true },
    });

    expect(await watchComputerDriver(driver.driver_id, "session:one", 960)).toBe(frame);
    expect(
      await controlComputerDriver(driver.driver_id, "session:one", "take_control"),
    ).toMatchObject({ holder: "human", requester_has_control: true });
    expect(mocks.unary.mock.calls).toEqual([
      [
        "computer.drivers.by_driver_id.watch.post",
        { driver_id: driver.driver_id },
        { session_id: "session:one", max_width: 960 },
      ],
      [
        "computer.drivers.by_driver_id.control.post",
        { driver_id: driver.driver_id },
        { session_id: "session:one", action: "take_control" },
      ],
    ]);
  });
});
