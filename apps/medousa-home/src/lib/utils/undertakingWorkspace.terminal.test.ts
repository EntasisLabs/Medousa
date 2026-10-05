import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ begin: vi.fn(), create: vi.fn(), activate: vi.fn(), sessions: vi.fn(), bind: vi.fn(), active: { workId: "work", executionRuntimeId: null, worktree: "/repo", baselineOid: "old", leaseId: null, boundTerminalSessionIds: [] as string[] } }));
vi.mock("$lib/forge", () => ({ canStartHumanEditing: () => true, startHumanEditingSession: api.begin }));
vi.mock("$lib/terminal", () => ({ terminalCreate: api.create, terminalSessions: api.sessions }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: { active: api.active, detail: null, setActiveFromItem: api.activate, bindTerminal: api.bind } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openTerminal: vi.fn() } }));
vi.mock("$lib/utils/codeWorkspaceController", () => ({ landCodeWorkingSet: vi.fn() }));
import { openProjectTerminal } from "./undertakingWorkspace";
import type { ItemProjection } from "$lib/forge";
beforeEach(() => { vi.clearAllMocks(); api.active.baselineOid = "old"; api.active.boundTerminalSessionIds = []; api.sessions.mockResolvedValue([]); api.create.mockResolvedValue({ session_id: "shell" }); });
it("reveals the empty terminal without creating a process or taking control", async () => {
  expect(await openProjectTerminal({ id: "work" } as ItemProjection, { create: false, activate: false })).toBeNull();
  expect(api.begin).not.toHaveBeenCalled(); expect(api.create).not.toHaveBeenCalled();
});
it("opens a human workspace shell without acquiring a lease", async () => {
  expect(await openProjectTerminal({ id: "work", allowed_actions: {} } as ItemProjection, { activate: false })).toBe("shell");
  expect(api.begin).not.toHaveBeenCalled();
  expect(api.create).toHaveBeenCalledWith({ work_id: "work", workspace_shell: true }, null);
  expect(api.bind).toHaveBeenCalledWith("shell");
});
it("does not reuse an agent or task process for a human terminal", async () => {
  api.active.boundTerminalSessionIds = ["agent"];
  api.sessions.mockResolvedValue([{ session_id: "agent", work_id: "work", root_kind: "forge" }]);
  expect(await openProjectTerminal({ id: "work" } as ItemProjection, { activate: false })).toBe("shell");
  expect(api.create).toHaveBeenCalledOnce();
});
it("reuses the human workspace shell without creating a process", async () => {
  api.sessions.mockResolvedValue([{ session_id: "existing", work_id: "work", root_kind: "workspace", workspace_context: { cwd: "/repo", current_branch: "new", attached_branch: "old" } }]);
  expect(await openProjectTerminal({ id: "work" } as ItemProjection, { activate: false })).toBe("existing");
  expect(api.create).not.toHaveBeenCalled();
});
it("does not reuse a shell whose folder no longer matches the project", async () => {
  api.sessions.mockResolvedValue([{ session_id: "old-folder", work_id: "work", root_kind: "workspace", workspace_context_error: "folder changed" }]);
  expect(await openProjectTerminal({ id: "work" } as ItemProjection, { activate: false })).toBe("shell");
  expect(api.create).toHaveBeenCalledOnce();
});
it("rejects shell discovery arriving for an earlier checkout of the same project", async () => {
  let resolve!: (value: unknown) => void;
  api.sessions.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const pending = openProjectTerminal({ id: "work" } as ItemProjection, { activate: false });
  api.active.baselineOid = "new";
  resolve([]);
  expect(await pending).toBeNull(); expect(api.create).not.toHaveBeenCalled();
});
it("does not bind a newly created shell after the checkout changes", async () => {
  let resolve!: (value: unknown) => void;
  api.create.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const pending = openProjectTerminal({ id: "work" } as ItemProjection, { activate: false });
  await vi.waitFor(() => expect(api.create).toHaveBeenCalledOnce());
  api.active.baselineOid = "new";
  resolve({ session_id: "late" });
  expect(await pending).toBeNull(); expect(api.bind).not.toHaveBeenCalled();
});
