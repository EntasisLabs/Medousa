import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ epoch: 1, cancel: vi.fn(), create: vi.fn(), clear: vi.fn(), config: vi.fn() }));
vi.mock("$lib/daemon", () => ({ cancelAgentSession: api.cancel, createAgentSession: api.create, getSessionAgentMode: async () => ({ effective_mode: "coder" }), getSessionCodeBinding: async () => ({ work_id: null }) }));
vi.mock("$lib/daemon/bot", () => ({ getSessionBot: async () => ({ binding: null }) }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { get workshopEpoch() { return api.epoch; }, focusedSessionId: "same-session" } }));
vi.mock("$lib/utils/sessionAgentRuntime", () => ({
  getSessionAgentRuntime: () => "pi", getSessionAgentSessionId: () => "old-provider-session", getSessionAgentWorkId: () => "old-work",
  clearSessionAgentSessionId: api.clear, setSessionAgentConfigOptions: api.config,
}));
import { prepareAgentSession } from "./prepareAgentSession";
beforeEach(() => { vi.clearAllMocks(); api.epoch = 1; });
it("does not clear the new workshop's metadata when cancellation finishes on the previous workshop", async () => {
  let resolve!: () => void;
  api.cancel.mockReturnValueOnce(new Promise<void>(done => { resolve = done; }));
  const pending = prepareAgentSession("same-session", "pi", { stopWhenUnbound: true });
  await vi.waitFor(() => expect(api.cancel).toHaveBeenCalledWith("old-provider-session"));
  api.epoch = 2; resolve();
  await expect(pending).rejects.toThrow("Workshop changed");
  expect(api.clear).not.toHaveBeenCalled(); expect(api.config).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
});
