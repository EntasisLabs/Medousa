import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PreparedAgentSession } from "./agentSessionController.svelte";

const state = vi.hoisted(() => ({ sessionId: "chat-a", runtime: "codex", bot: false, external: false, seeded: false }));
const api = vi.hoisted(() => ({ bot: vi.fn(), prompt: vi.fn(), ticket: vi.fn(), begin: vi.fn(), seed: vi.fn(), runtimeBot: vi.fn(), fields: vi.fn(), options: vi.fn(), clearMedia: vi.fn() }));
vi.mock("./runtimeBotTurns.svelte", () => ({sendRuntimeBotTurn:api.runtimeBot}));
vi.mock("$lib/daemon", () => ({ createTurnTicket: api.ticket, promptAgentSession: api.prompt }));
vi.mock("$lib/daemon/bot", () => ({ getSessionBot: api.bot }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: {
  get sessionId() { return state.sessionId; }, get focusedSessionId() { return state.sessionId; },
  workshopEpoch: 1, workshopScopeId: "workshop-a", pendingMediaRefs: [],
  messagesFor: () => [{ id: "old", role: "user", content: "Use Svelte 5" }],
  withSessionFields: (id: string, run: () => void) => { api.fields(id); run(); }, beginTurn: api.begin, clearPendingMedia: api.clearMedia, startTurnStream: vi.fn(),
} }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { forSession: () => state.bot ? {bot_id:"ada",external_agent:state.external ? {runtime:"hermes"} : null} : null } }));
vi.mock("$lib/stores/userProfiles.svelte", () => ({ userProfiles: { turnIdentityUserId: () => "owner" } }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { turnSelection: () => null } }));
vi.mock("$lib/stores/voicePresets.svelte", () => ({ voicePresets: { turnVoiceFields: () => ({}) } }));
vi.mock("$lib/interactiveTurnOptions", () => ({ prepareInteractiveTurnOptions: api.options }));
vi.mock("$lib/utils/undertakingWorkspace", () => ({ activeCodeContext: () => null }));
vi.mock("$lib/liquid/surfaces/chat/chatInteractions", () => ({ chatInteractions: { envelopes: () => [], ack: vi.fn() } }));
vi.mock("$lib/liquid/observability", () => ({ recordLiquidMetric: vi.fn(), recordLiquidPresentationOpportunity: vi.fn() }));
vi.mock("$lib/utils/sessionAgentRuntime", async (original) => ({
  ...await original<object>(), getSessionAgentRuntime: () => state.runtime,
  agentConversationContextSeeded: () => state.seeded,
  markAgentConversationContextSeeded: api.seed,
}));
import { submitChatTurn } from "./submitTurnController";

const prepared: PreparedAgentSession = { agentSessionId: "agent-a", streamUrl: "/stream", streamReady: true, acceptedAt: "2026-10-01T12:00:00Z" };
function input(synchronize = vi.fn(async () => prepared)) {
  return { userContent: "Continue", prompt: "Continue", mode: "interactive" as const,
    synchronizeAgentSession: synchronize, onAgentSessionLost: vi.fn(), scrollToLatest: vi.fn() };
}
beforeEach(() => {
  vi.clearAllMocks(); state.sessionId = "chat-a"; state.runtime = "codex"; state.bot = false; state.external = false; state.seeded = false;
  api.runtimeBot.mockResolvedValue({ticket:{jobId:"job",turnId:"turn"}});
  api.bot.mockResolvedValue({ binding: null }); api.prompt.mockResolvedValue({});
  api.ticket.mockResolvedValue({ turn_id: "native", session_id: "chat-a", stream_url: "/stream" });
  api.options.mockResolvedValue({ provider: "chatgpt", model: "gpt" });
  api.seed.mockImplementation(() => { state.seeded = true; });
});

describe("session identity at turn submission", () => {
  it("uses native Bot routing despite a stale Codex preference", async () => {
    api.bot.mockResolvedValue({ binding: { bot_id: "ada" } });
    const request = input();
    await submitChatTurn(request);
    expect(request.synchronizeAgentSession).not.toHaveBeenCalled();
    expect(api.prompt).not.toHaveBeenCalled();
    expect(api.ticket).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "chat-a", prompt: "Continue" }));
  });
  it("invokes the enrolled Bot instead of a native or freely selected ACP runtime", async () => {
    state.bot = true; state.external = true;
    const request = input(); await submitChatTurn(request);
    expect(api.runtimeBot).toHaveBeenCalledWith(expect.objectContaining({bot_id:"ada"}),"chat-a","Continue");
    expect(request.synchronizeAgentSession).not.toHaveBeenCalled();expect(api.ticket).not.toHaveBeenCalled();expect(api.prompt).not.toHaveBeenCalled();
  });
  it("rejects native turn submission for an attached provider conversation", async () => {
    state.runtime = "muse";
    await expect(submitChatTurn(input())).rejects.toThrow("Provider conversations");
    expect(api.ticket).not.toHaveBeenCalled(); expect(api.prompt).not.toHaveBeenCalled();
  });
  it("hands off conversation context once per runtime process", async () => {
    await submitChatTurn(input());
    expect(api.prompt).toHaveBeenLastCalledWith("agent-a", expect.stringContaining("Use Svelte 5"), null);
    expect(api.seed).toHaveBeenCalledWith("chat-a", "agent-a");
    await submitChatTurn(input());
    expect(api.prompt).toHaveBeenLastCalledWith("agent-a", "Continue", null);
  });
  it("does not send an old chat's prompt after the user switches chats during runtime startup", async () => {
    const synchronize = vi.fn(async () => { state.sessionId = "chat-b"; return prepared; });
    await expect(submitChatTurn(input(synchronize))).rejects.toThrow("Conversation changed");
    expect(api.begin).not.toHaveBeenCalled(); expect(api.prompt).not.toHaveBeenCalled();
  });
});

it("addresses another native session using its model context and leaves the focused chat attachments alone", async () => {
  state.runtime = "medousa";
  await submitChatTurn({ ...input(), sessionId: "labbing" });
  expect(api.bot).toHaveBeenCalledWith("labbing");
  expect(api.options).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "labbing", workshopScopeId: "workshop-a" }));
  expect(api.ticket).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "labbing", mediaRefs: [] }));
  expect(api.fields).toHaveBeenCalledWith("labbing"); expect(api.clearMedia).not.toHaveBeenCalled();
  expect(state.sessionId).toBe("chat-a");
});
it("keeps an explicit agent destination when the visible chat changes during preparation", async () => {
  const synchronize = vi.fn(async () => { state.sessionId = "chat-b"; return prepared; });
  await submitChatTurn({ ...input(synchronize), sessionId: "labbing" });
  expect(synchronize).toHaveBeenCalledWith("labbing", "codex", expect.anything());
  expect(api.fields).toHaveBeenCalledWith("labbing"); expect(api.prompt).toHaveBeenCalled();
});
