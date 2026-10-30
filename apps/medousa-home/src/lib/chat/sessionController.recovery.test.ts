import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { getSessionHistory } from "$lib/daemon";
import { ensureSessionHydrated, reloadCurrentSession, switchSession } from "./sessionController";
import { emptySessionRuntime } from "./chatSessionRuntime";
import type { ChatStoreHost } from "./chatStoreHost";
import type { ChatMessage } from "$lib/types/chat";
import type { SessionHistoryResponse } from "$lib/types/session";
const scenes = vi.hoisted(() => ({ reset: vi.fn() }));
vi.mock("$lib/daemon", () => ({ deriveSession: vi.fn(), getSessionHistory: vi.fn(), deleteSession: vi.fn(), setSessionDisplayName: vi.fn() }));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: { saveActiveSession: vi.fn() } }));
vi.mock("$lib/liquid/surfaces/chat/chatScenes.svelte", () => ({ chatScenes: scenes }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("localStorage", { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn() });
});
afterEach(() => vi.unstubAllGlobals());
function host() {
  const state = {
    workshopEpoch: 1, workshopScopeId: "personal", sessionId: "chat", transcriptEpoch: 1,
    messages: [] as ChatMessage[], sessionPristine: false, historyLoading: false,
    sessionRuntimes: new Map(), tryReattachActiveTurn: vi.fn(async () => false),
    sanitizeTranscript: vi.fn(), noteResumeFailure: vi.fn(),
    stashFocusedRuntime: vi.fn(), flushDraftPersist: vi.fn(),
    loadRuntimeIntoFocused(runtime: ReturnType<typeof emptySessionRuntime>) {
      state.sessionId = runtime.sessionId; state.messages = runtime.messages;
      state.transcriptEpoch = runtime.transcriptEpoch;
    },
  };
  return state as unknown as ChatStoreHost;
}
function history(): SessionHistoryResponse {
  return { session_id: "chat", authority_id: "authority", turns: [{
    role: "user", content: "Hi", content_digest: "digest", entry_id: "user", entry_seq: 1,
    timestamp: "2026-10-30T00:00:00Z", tool_names: [],
  }], next_cursor: null };
}
it("reattaches when an empty session hydrates after refresh/reopen", async () => {
  const state = host();
  vi.mocked(getSessionHistory).mockResolvedValue(history());
  await ensureSessionHydrated(state);
  expect(state.messages[0].content).toBe("Hi");
  expect(state.tryReattachActiveTurn).toHaveBeenCalledOnce();
  expect(state.historyLoading).toBe(false);
});
it("preserves text arriving while a history reload is in flight", async () => {
  const state = host();
  state.messages = [{ id: "live", role: "assistant", turnId: "turn", streaming: true, content: "Hello" }];
  vi.mocked(getSessionHistory).mockImplementationOnce(async () => {
    state.messages = [{ ...state.messages[0], content: "Hello world" }];
    return { ...history(), turns: [{ ...history().turns[0], role: "assistant", content: "Hello", caused_by: { authority_id: "authority", session_id: "chat", execution_id: "turn" } }] } as SessionHistoryResponse;
  });
  await reloadCurrentSession(state);
  expect(state.messages).toHaveLength(1);
  expect(state.messages[0]).toMatchObject({ id: "live", streaming: true, content: "Hello world" });
  expect(state.tryReattachActiveTurn).toHaveBeenCalledOnce();
});
it("does not reattach a stale reload after switching away", async () => {
  const state = host();
  vi.mocked(getSessionHistory).mockImplementationOnce(async () => {
    state.sessionId = "other";
    return history();
  });
  await reloadCurrentSession(state);
  expect(state.messages).toEqual([]);
  expect(state.tryReattachActiveTurn).not.toHaveBeenCalled();
});
it("reopens the focused session with reattachment rather than history only", async () => {
  const state = host();
  vi.mocked(getSessionHistory).mockResolvedValue(history());
  await switchSession(state, "chat");
  expect(state.tryReattachActiveTurn).toHaveBeenCalledOnce();
  expect(scenes.reset).not.toHaveBeenCalled();
});
it("keeps retained scene state when opening a cached live session", async () => {
  const state = host();
  const cached = emptySessionRuntime("cached");
  cached.messages = [{ id: "answer", role: "assistant", turnId: "live", streaming: true, content: "Working" }];
  state.sessionRuntimes.set("cached", cached);
  vi.mocked(getSessionHistory).mockResolvedValue({ ...history(), turns: [] });
  await switchSession(state, "cached");
  await vi.waitFor(() => expect(state.tryReattachActiveTurn).toHaveBeenCalledOnce());
  expect(scenes.reset).not.toHaveBeenCalled();
});
