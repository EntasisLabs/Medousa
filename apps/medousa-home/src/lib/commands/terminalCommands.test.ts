/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import type { WorkshopCommandContext } from "./types";
const api = vi.hoisted(() => ({ create: vi.fn(), ssh: vi.fn(), tab: vi.fn(), navigate: vi.fn(), close: vi.fn(), scope: "mac", desktop: true }));
vi.mock("$lib/platform", () => ({ isTauriDesktop: () => api.desktop }));
vi.mock("$lib/terminal", () => ({ terminalCreate: api.create }));
vi.mock("$lib/utils/sshApi", () => ({ sshAction: api.ssh }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => api.scope }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openTerminal: api.tab } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { navigateDesktop: api.navigate } }));
import { buildTerminalCommands, buildSshTerminalCommands } from "./terminalCommands";
import { filterAndSortCommands } from "./score";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
const target = { target_id: "ssh-prodesk", name: "ProDesk", username: "ops", host: "10.12.0.16", port: 2222, agent_access: false };
const ctx = { callbacks: { close: api.close } } as unknown as WorkshopCommandContext;
beforeEach(() => { vi.resetAllMocks(); api.scope = "mac"; api.desktop = true; commandSpotlight.closeSpotlight(); });
it("opens a plain shell on Connection without inheriting a Code worker or project", async () => {
  api.create.mockResolvedValue({ session_id: "plain-shell" });
  await buildTerminalCommands()[0].run(ctx);
  expect(api.create).toHaveBeenCalledWith({ work_id: null, cwd: null }, null);
  expect(api.tab).toHaveBeenCalledWith("plain-shell", { title: "Terminal", executionRuntimeId: null });
  expect(api.close).toHaveBeenCalledOnce();
});
it("enters the SSH picker without creating a session", async () => {
  await buildTerminalCommands()[1].run(ctx);
  expect(commandSpotlight.mode).toBe("ssh"); expect(commandSpotlight.open).toBe(true); expect(commandSpotlight.seedQuery).toBe("");
  expect(api.ssh).not.toHaveBeenCalled(); expect(api.close).not.toHaveBeenCalled();
  commandSpotlight.leaveSshTargets(); expect(commandSpotlight.mode).toBe("default"); expect(commandSpotlight.open).toBe(true);
});
it.each(["ssh", "prodesk", "10.12.0.16", "ops", "2222"])("finds saved SSH servers by %s", (query) => {
  const hits = filterAndSortCommands(buildSshTerminalCommands([target], "mac"), query);
  expect(hits.map((hit) => hit.id)).toEqual(["terminal-ssh:ssh-prodesk"]);
});
it("opens the saved SSH target even when agent access is off", async () => {
  api.ssh.mockResolvedValue({ session_id: "ssh-session", error: null });
  await buildSshTerminalCommands([target], "mac")[0].run(ctx);
  expect(api.ssh).toHaveBeenCalledWith("terminal", { target_id: "ssh-prodesk", request_key: expect.any(String) });
  expect(api.tab).toHaveBeenCalledWith("ssh-session", { title: "SSH · ProDesk", executionRuntimeId: null }); expect(api.close).toHaveBeenCalledOnce();
});
it.each(["hp-prodesk-01", "10.12.0.16"])("ranks the exact server %s ahead of similar names and addresses", (query) => {
  const commands = buildSshTerminalCommands([
    { ...target, target_id: "ssh-prodesk-02", name: "hp-prodesk-02", host: "10.12.0.17" },
    { ...target, target_id: "ssh-prodesk-01", name: "hp-prodesk-01" },
  ], "mac");
  expect(filterAndSortCommands(commands, query)[0].id).toBe("terminal-ssh:ssh-prodesk-01");
});
it("leaves Spotlight open and does not create a tab when the connection fails", async () => {
  api.ssh.mockResolvedValue({ session_id: null, error: "Server unavailable" });
  await expect(buildSshTerminalCommands([target], "mac")[0].run(ctx)).rejects.toThrow("Server unavailable");
  expect(api.close).not.toHaveBeenCalled(); expect(api.tab).not.toHaveBeenCalled();
});
it("rejects a stale result before starting a terminal on a different workshop", async () => {
  const command = buildSshTerminalCommands([target], "mac")[0]; api.scope = "remote";
  await expect(command.run(ctx)).rejects.toThrow("workshop changed"); expect(api.ssh).not.toHaveBeenCalled();
});
it("does not attach a completed request to a newly selected workshop", async () => {
  api.create.mockImplementation(async () => { api.scope = "remote"; return { session_id: "old-shell" }; });
  await expect(buildTerminalCommands()[0].run(ctx)).rejects.toThrow("workshop changed"); expect(api.tab).not.toHaveBeenCalled(); expect(api.close).not.toHaveBeenCalled();
});
it("keeps terminal launch commands off mobile and browser surfaces", () => {
  api.desktop = false; expect(buildTerminalCommands()).toEqual([]); expect(buildSshTerminalCommands([target], "mac")).toEqual([]);
});
