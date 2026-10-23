import { beforeEach, expect, it, vi } from "vitest";
const { invoke, browser } = vi.hoisted(() => ({
  invoke: vi.fn().mockResolvedValue({}),
  browser: { value: false },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("$lib/platform", () => ({ isBrowserWorkshop: () => browser.value }));
vi.mock("$lib/wasm/browserDaemon", () => ({ runBrowserGrapheme: vi.fn() }));
vi.mock("$lib/wasm/browserPortal", () => ({
  browserPortalActive: () => false,
  portalOperation: vi.fn(),
}));
vi.mock("$lib/utils/workshopLocality", () => ({
  activeWorkshopId: () => "personal",
}));
import { daemonUnary } from "./contractClient";
beforeEach(() => {
  invoke.mockClear();
  browser.value = false;
});

it("routes Personal by client workshop scope rather than daemon runtime identity", async () => {
  await daemonUnary(
    "create.projects.get",
    {},
    undefined,
    null,
    { limit: "50" },
    "personal",
  );
  expect(invoke).toHaveBeenCalledWith("daemon_unary", {
    operation: "create.projects.get",
    pathParams: {},
    body: undefined,
    executionRuntimeId: null,
    query: { limit: "50" },
    workshopScopeId: "personal",
  });
});

it("preserves existing execution-runtime overrides without adding a workshop scope", async () => {
  await daemonUnary("create.projects.get", {}, undefined, "daemon-remote");
  expect(invoke.mock.calls[0][1]).toMatchObject({
    executionRuntimeId: "daemon-remote",
  });
  expect(invoke.mock.calls[0][1]).not.toHaveProperty("workshopScopeId");
});

it("cannot redirect a retained browser project after a workshop switch", async () => {
  browser.value = true;
  await expect(
    daemonUnary(
      "create.projects.get",
      {},
      undefined,
      null,
      undefined,
      "previous-workshop",
    ),
  ).rejects.toThrow("no longer active");
  expect(invoke).not.toHaveBeenCalled();
});
