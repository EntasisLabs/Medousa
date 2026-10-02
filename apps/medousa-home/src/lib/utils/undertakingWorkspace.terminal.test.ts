import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ begin: vi.fn(), create: vi.fn(), activate: vi.fn(), active: { workId: "work", executionRuntimeId: null, worktree: "/repo", baselineOid: "old", leaseId: null, boundTerminalSessionIds: [] as string[] } }));
vi.mock("$lib/forge", () => ({ canStartHumanEditing: () => true, startHumanEditingSession: api.begin }));
vi.mock("$lib/terminal", () => ({ terminalCreate: api.create }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: { active: api.active, detail: null, setActiveFromItem: api.activate } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openTerminal: vi.fn() } }));
vi.mock("$lib/utils/codeWorkspaceController", () => ({ landCodeWorkingSet: vi.fn() }));
import { openTrackedTerminal } from "./undertakingWorkspace";
import type { ItemProjection } from "$lib/forge";
beforeEach(() => { vi.clearAllMocks(); api.active.baselineOid = "old"; api.active.boundTerminalSessionIds = []; });
it("reveals the empty terminal without creating a process or taking control", async () => {
  expect(await openTrackedTerminal({ id: "work" } as ItemProjection, { create: false, activate: false })).toBeNull();
  expect(api.begin).not.toHaveBeenCalled(); expect(api.create).not.toHaveBeenCalled();
});
it("rejects editing control arriving for an earlier checkout of the same project", async () => {
  let resolve!: (value: unknown) => void;
  api.begin.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const pending = openTrackedTerminal({ id: "work", allowed_actions: {} } as ItemProjection, { activate: false });
  api.active.baselineOid = "new";
  resolve({ item: { id: "work" }, lease: { lease_id: "old", generation: 1 } });
  expect(await pending).toBeNull(); expect(api.activate).not.toHaveBeenCalled(); expect(api.create).not.toHaveBeenCalled();
});
