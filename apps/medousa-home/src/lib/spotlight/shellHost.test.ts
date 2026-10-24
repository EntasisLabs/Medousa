/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({ scope: "a", tab: "file-a", command: vi.fn(), shell: vi.fn(), native: vi.fn(), collect: vi.fn(), lenses: vi.fn(), refresh: vi.fn(), activity: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => state.scope }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { get workshopScopeId() { return state.scope; }, sessionId: "existing", refreshSessions: state.refresh } }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [1], refreshNotes: state.refresh } }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { refresh: state.refresh } }));
vi.mock("$lib/stores/connectedAgents.svelte", () => ({ connectedAgents: { refresh: state.refresh } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [1], refreshModulesAndScripts: state.refresh } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { desktops: [], activeDesktopId: "desk", activeGroupId: "pane", get activeTab() { return { id: state.tab }; } } }));
vi.mock("$lib/stores/spotlightPins.svelte", () => ({ spotlightPins: { get: () => ({ kind: "chat", target: "pinned", label: "Pinned chat" }) } }));
vi.mock("$lib/commands/context", () => ({ buildWorkshopCommandContext: () => ({ callbacks: {}, navigate: vi.fn() }) }));
vi.mock("$lib/commands/collectCommands", () => ({ collectWorkshopCommands: state.collect, collectWorkshopLensCommands: state.lenses }));
vi.mock("$lib/commands/pinCommands", () => ({ buildPinnedJumpCommands: () => [{ id: "pin-jump:0", label: "Pinned", run: state.command }] }));
vi.mock("$lib/commands/runWorkshopCommand", () => ({ executeWorkshopCommand: async (ctx: unknown, command: { run: (ctx: unknown) => Promise<void> }) => command.run(ctx) }));
vi.mock("$lib/runtime/shellUseCases", () => ({ focusChatComposer: vi.fn() }));
vi.mock("$lib/desktop/windows", () => ({ openDesktopWindow: state.native }));
vi.mock("$lib/window", () => ({ showMainWindow: state.shell }));
vi.mock("$lib/utils/workChromeEvents", () => ({ openNewAsk: vi.fn() }));
vi.mock("$lib/utils/sshApi", () => ({ sshTargets: async () => [] }));
vi.mock("$lib/utils/terminalActions", () => ({ openSshTerminal: vi.fn() }));
vi.mock("./activityNavigation", () => ({ openActivityTarget: state.activity }));
vi.mock("./desktopGroups", () => ({ spotlightWorkspaceLayout: () => null, collectDesktopGroups: vi.fn(), spotlightTabKind: vi.fn(), spotlightTabPreview: vi.fn() }));
import { createSpotlightShellHost } from "./shellHost";
import type { SpotlightSnapshot } from "./host";
const query = { query: ">run", mode: "default" as const, lens: "overview" as const, scopeId: "home" };
beforeEach(() => {
  vi.clearAllMocks(); state.scope = "a"; state.tab = "file-a";
  const groups = [{ section: "do", label: "Actions", commands: [{ id: "workbench.action.test", label: "Test", section: "do", run: state.command }] }];
  state.collect.mockReturnValue(groups); state.lenses.mockReturnValue(groups); state.shell.mockResolvedValue(undefined); state.native.mockResolvedValue("window");
});
async function ticket(host: ReturnType<typeof createSpotlightShellHost>) {
  const snapshot = await host.handle("a", { kind: "collect", query }) as SpotlightSnapshot;
  return snapshot.groups[0].commands[0];
}
it("uses the shared query and lens builders and sends metadata without executable functions", async () => {
  const host = createSpotlightShellHost();
  const command = await ticket(host);
  expect(state.collect).toHaveBeenCalledWith(expect.anything(), expect.objectContaining({ query: ">run" }));
  expect(command).not.toHaveProperty("run"); expect(command.token).toBeTruthy();
  await host.handle("a", { kind: "collect", query: { ...query, query: "", lens: "actions" } });
  expect(state.lenses).toHaveBeenCalledWith(expect.anything(), "actions");
});
it("executes in the actual shell, delivers prompt arguments and consumes a command once", async () => {
  const host = createSpotlightShellHost(), command = await ticket(host);
  await host.handle("a", { kind: "execute", token: command.token, args: "custom title" });
  expect(state.shell).toHaveBeenCalledOnce(); expect(state.command).toHaveBeenCalledWith(expect.anything(), "custom title");
  expect(state.shell.mock.invocationCallOrder[0]).toBeLessThan(state.command.mock.invocationCallOrder[0]);
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("expired");
  expect(state.command).toHaveBeenCalledOnce();
});
it("rejects stale workshop and document context instead of running on another selection", async () => {
  const host = createSpotlightShellHost(), command = await ticket(host);
  state.tab = "file-b";
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("document changed");
  state.scope = "b";
  await expect(host.handle("a", { kind: "collect", query })).rejects.toThrow("changed workshops");
  expect(state.command).not.toHaveBeenCalled(); expect(state.shell).not.toHaveBeenCalled();
});
it("rechecks document context after the native focus handoff", async () => {
  const host = createSpotlightShellHost(), command = await ticket(host);
  state.shell.mockImplementationOnce(async () => { state.tab = "different-file"; });
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("document changed");
  expect(state.command).not.toHaveBeenCalled();
});
it("opens concrete conversations as native windows without mutating main-shell navigation", async () => {
  state.collect.mockReturnValue([{ section: "open", label: "Open", commands: [{ id: "open-session:saved", label: "Chat", run: state.command }] }]);
  const host = createSpotlightShellHost(), command = await ticket(host);
  await host.handle("a", { kind: "execute", token: command.token });
  expect(state.native).toHaveBeenCalledWith({ kind: "chat", workshopId: "a", resourceId: "saved", title: "Chat" });
  expect(state.command).not.toHaveBeenCalled(); expect(state.shell).not.toHaveBeenCalled();
});
it("propagates execution failures and never retries a consumed action", async () => {
  state.command.mockRejectedValueOnce(new Error("Could not save"));
  const host = createSpotlightShellHost(), command = await ticket(host);
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("Could not save");
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("expired");
  expect(state.command).toHaveBeenCalledOnce();
});
it("clears command tickets on teardown and rejects empty pin slots", async () => {
  const host = createSpotlightShellHost(), command = await ticket(host);
  host.dispose();
  await expect(host.handle("a", { kind: "execute", token: command.token })).rejects.toThrow("owner closed");
  const current = createSpotlightShellHost();
  await expect(current.handle("a", { kind: "pin", slot: -1 })).rejects.toThrow("Invalid pin");
  await expect(current.handle("a", { kind: "pin", slot: 1 })).rejects.toThrow("empty");
});
