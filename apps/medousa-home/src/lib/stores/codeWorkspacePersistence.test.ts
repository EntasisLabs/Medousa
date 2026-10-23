import { afterEach, beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ save: vi.fn(), state: vi.fn(), source: vi.fn() }));
vi.mock("$lib/forge", async original => ({ ...await original<typeof import("$lib/forge")>(), saveCodeWorkspaceState: api.save, getCodeWorkspaceState: api.state, getUndertakingSource: api.source }));
import { codeWorkspace } from "./codeWorkspace.svelte";

beforeEach(async () => {
  vi.useFakeTimers(); codeWorkspace.resetForWorkshopSwitch();
  api.save.mockReset().mockResolvedValue({}); api.state.mockResolvedValue({ tabs: [] });
  api.source.mockResolvedValue({ work_id: "project", path: "file.ts", content: "base", digest: "digest", byte_size: 4 });
  await codeWorkspace.hydrate("project");
  await codeWorkspace.open("project", "file.ts", null, { persist: false });
  codeWorkspace.setLease("project", { lease_id: "lease", generation: 1 });
});
afterEach(() => { codeWorkspace.resetForWorkshopSwitch(); vi.useRealTimers(); });
it("checkpoints recent draft edits before the debounce has fired", async () => {
  const tab = codeWorkspace.activeFor("project")!;
  codeWorkspace.updateDraft(tab.tabId, "last keystrokes");
  expect(await codeWorkspace.flushBeforeWorkshopSwitch()).toBe(true);
  expect(api.save).toHaveBeenCalledWith("project", expect.objectContaining({ tabs: [expect.objectContaining({ draft: "last keystrokes", source_digest: "digest" })] }), { lease_id: "lease", generation: 1 });
  expect(tab.content).toBe("base");
});
it("serializes older persistence before the final switch checkpoint", async () => {
  const tab = codeWorkspace.activeFor("project")!;
  codeWorkspace.updateDraft(tab.tabId, "older");
  let finish!: (value: unknown) => void;
  api.save.mockImplementationOnce(() => new Promise(resolve => finish = resolve));
  const first = codeWorkspace.persist("project"); await Promise.resolve();
  codeWorkspace.updateDraft(tab.tabId, "newer");
  const flush = codeWorkspace.flushBeforeWorkshopSwitch();
  expect(api.save).toHaveBeenCalledTimes(1);
  finish({}); await first; expect(await flush).toBe(true);
  expect(api.save.mock.calls[1][1].tabs[0].draft).toBe("newer");
});
it("keeps the current draft available if its checkpoint fails", async () => {
  const tab = codeWorkspace.activeFor("project")!;
  codeWorkspace.updateDraft(tab.tabId, "keep me"); api.save.mockRejectedValueOnce(new Error("offline"));
  expect(await codeWorkspace.flushBeforeWorkshopSwitch()).toBe(false);
  expect(codeWorkspace.activeFor("project")?.draft).toBe("keep me");
});
