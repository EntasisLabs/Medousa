import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ unary: vi.fn(), invoke: vi.fn(), local: false }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: api.unary }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: api.invoke }));
vi.mock("$lib/platform", () => ({ isTauriDesktop: () => true }));
vi.mock("$lib/utils/workshopLocality", () => ({ isCoLocatedWorkshop: () => api.local, activeWorkshopId: () => "connected" }));
import { probeAccountConnections, beginTerminalLogin, installAccountCli } from "./accountConnections";
beforeEach(() => { vi.clearAllMocks(); api.local = false; });
it("reports the connected workshop status without substituting local credentials", async () => {
  api.unary.mockResolvedValue({ runtimes: [{ runtime: "claude", available: true, binary_present: true, auth_status: "signed_out", auth_detail: "Sign in on this host" }, { runtime: "pi", available: true, binary_present: true }] });
  const status = await probeAccountConnections();
  expect(status.claude.authStatus).toBe("signed_out");
  expect(status.pi.authStatus).toBe("unknown");
  expect(status.hades.binaryPresent).toBe(false);
  expect(api.invoke).not.toHaveBeenCalled();
});
it("blocks setup actions on a remote workshop before invoking a local process", async () => {
  await expect(beginTerminalLogin("cursor")).rejects.toThrow("workshop host");
  await expect(installAccountCli("chatgpt")).rejects.toThrow("workshop host");
  expect(api.invoke).not.toHaveBeenCalled();
});
it("retains official local setup for existing agents", async () => {
  api.local = true;
  await beginTerminalLogin("cursor");
  expect(api.invoke).toHaveBeenCalledWith("account_begin_terminal_login", { account: "cursor" });
});
