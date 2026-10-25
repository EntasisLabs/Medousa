import { beforeEach, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({ scope: "local", health: vi.fn() }));
vi.mock("$lib/daemon/runtime", () => ({ checkDaemonHealth: state.health }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => state.scope }));
import { ensureReferenceSupport } from "./referenceSupport";
const context = { source: "notes", references: [{ kind: "vault_note" as const, root_id: "obsidian", path: "same.md", title: "Spec" }] };
beforeEach(() => { vi.clearAllMocks(); state.scope = "local"; });
it("rejects an older engine that would silently ignore exact vault identity", async () => {
  state.health.mockResolvedValue({ runtime: { advertised_capabilities: [] } });
  await expect(ensureReferenceSupport(context)).rejects.toThrow("Update the workshop engine");
  state.health.mockResolvedValue({ runtime: { advertised_capabilities: ["turn.resource_references.v1"] } });
  await expect(ensureReferenceSupport(context)).resolves.toBeUndefined();
});
it("does not reroute a reference when the workshop changes during the capability probe", async () => {
  state.health.mockImplementationOnce(async () => { state.scope = "other"; return { runtime: { advertised_capabilities: ["turn.resource_references.v1"] } }; });
  await expect(ensureReferenceSupport(context)).rejects.toThrow("workshop changed");
});
it("leaves ordinary and legacy editor-context messages compatible", async () => {
  await ensureReferenceSupport(null); await ensureReferenceSupport({ source: "vscode" });
  expect(state.health).not.toHaveBeenCalled();
});
