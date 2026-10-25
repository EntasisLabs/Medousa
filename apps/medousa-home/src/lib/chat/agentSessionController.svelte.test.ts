/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import { flushSync } from "svelte";
const api = vi.hoisted(() => ({ epoch: 1, runtime: "medousa", cancel: vi.fn(), clear: vi.fn(), setRuntime: vi.fn(), prepare: vi.fn(), config: vi.fn(), cacheConfig: vi.fn() }));
vi.mock("$lib/daemon", () => ({ cancelAgentSession: api.cancel, setAgentSessionConfigOption: api.config }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { forSession: () => null, refreshSessionBinding: async () => {} } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { focusedSessionId: "same-session", workshopScopeId: "studio", get workshopEpoch() { return api.epoch; } } }));
vi.mock("./prepareAgentSession", () => ({ prepareAgentSession: api.prepare }));
vi.mock("$lib/utils/sessionAgentRuntime", () => ({
  getSessionAgentRuntime: () => api.runtime, getSessionAgentSessionId: () => "previous-provider", getSessionAgentConfigOptions: () => [], getSessionAgentWorkId: () => "previous-work",
  isExternalAgentRuntime: (value: string) => value !== "medousa", isProviderConversationRuntime: () => false,
  setSessionAgentRuntime: (session: string, runtime: string) => { api.runtime = runtime; api.setRuntime(session, runtime); },
  clearSessionAgentSessionId: api.clear, setSessionAgentConfigOptions: api.cacheConfig,
}));
import { createAgentSessionController } from "./agentSessionController.svelte";
beforeEach(() => { vi.clearAllMocks(); api.epoch = 1; api.runtime = "medousa"; });
function controller() {
  let result!: ReturnType<typeof createAgentSessionController>;
  const dispose = $effect.root(() => { result = createAgentSessionController(); });
  flushSync();
  return { result, dispose };
}
it("does not send queued cancellation to a newly selected workshop", async () => {
  const { result, dispose } = controller();
  try {
    result.onRuntimeChange("claude"); api.epoch = 2;
    await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
    expect(api.cancel).not.toHaveBeenCalled(); expect(api.prepare).not.toHaveBeenCalled();
  } finally { dispose(); }
});
it("does not clear or roll back new workshop state after an old cancellation completes", async () => {
  let resolve!: () => void;
  api.cancel.mockReturnValueOnce(new Promise<void>(done => { resolve = done; }));
  const { result, dispose } = controller();
  try {
    result.onRuntimeChange("claude");
    await vi.waitFor(() => expect(api.cancel).toHaveBeenCalledOnce());
    api.epoch = 2; resolve();
    await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
    expect(api.clear).not.toHaveBeenCalled(); expect(api.prepare).not.toHaveBeenCalled();
    expect(api.setRuntime).toHaveBeenCalledTimes(1);
  } finally { dispose(); }
});

it("does not apply an old workshop's model controls after switching workshops", async () => {
  let resolve!: (value: unknown) => void;
  api.config.mockReturnValueOnce(new Promise(done => { resolve = done; }));
  const { result, dispose } = controller();
  try {
    const pending = result.updateAgentConfig("model", "new-model");
    api.epoch = 2; resolve({ config_options: [{ id: "model", currentValue: "old-host-model" }] });
    await pending;
    expect(api.cacheConfig).not.toHaveBeenCalled();
  } finally { dispose(); }
});
