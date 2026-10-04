import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("$lib/window", () => ({ isTauri: () => true }));
vi.mock("$lib/daemon", () => ({
  getDaemonUrl: async () => "http://unused",
  operationPath: (operation: string) => {
    if (operation === "forge.items.start.post") return "/v1/forge/items/start";
    throw new Error(`Unexpected creation operation: ${operation}`);
  },
}));
vi.mock("$lib/executionAuthority", () => ({
  getCoderExecutionTransport: () => "selected-workshop",
}));

import { startUndertaking } from "$lib/forge";

describe("undertaking creation transport", () => {
  beforeEach(() => invoke.mockReset());

  it("keeps an exact retry key and selected workshop after an uncertain timeout", async () => {
    const input = {
      title: "Penjamin", brief: "Scaffold the app", repo_path: "/work/penjamin",
      base_ref: "main", workspace_mode: "attached_checkout" as const,
      request_key: "create-penjamin-once",
    };
    invoke.mockRejectedValueOnce("request timed out; outcome unknown");
    await expect(startUndertaking(input)).rejects.toThrow("timed out");
    invoke.mockResolvedValueOnce({ id: "work-one", workspace_mode: "attached_checkout" });
    await expect(startUndertaking(input)).resolves.toMatchObject({ id: "work-one" });
    expect(invoke).toHaveBeenCalledTimes(2);
    for (const call of invoke.mock.calls) {
      expect(call).toEqual(["forge_request", {
        method: "POST", path: "/v1/forge/items/start",
        body: input, executionRuntimeId: "selected-workshop",
      }]);
    }
  });

  it("does not degrade current-checkout creation into a separate legacy registration", async () => {
    invoke.mockRejectedValueOnce("workshop returned HTTP 404 Not Found:");
    await expect(startUndertaking({
      title: "App", brief: "Scaffold", repo_path: "/work/app",
      workspace_mode: "attached_checkout", request_key: "once",
    })).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
