import { afterEach, describe, expect, it, vi } from "vitest";
import { getSessionHistory } from "$lib/daemon";
import { switchSession } from "$lib/chat/sessionController";
import { emptySessionRuntime } from "$lib/chat/chatSessionRuntime";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import type { ChatMessage } from "$lib/types/chat";
import type { SessionHistoryResponse } from "$lib/types/session";

vi.mock("$lib/daemon", () => ({
  deriveSession: vi.fn(),
  getSessionHistory: vi.fn(),
  listSessions: vi.fn(),
  deleteSession: vi.fn(),
  setSessionDisplayName: vi.fn(),
}));

vi.mock("$lib/stores/workshops.svelte", () => ({
  workshops: { saveActiveSession: vi.fn(async () => {}) },
}));

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("cached session resume", () => {
  it("merges durable results when opening a cached source session", async () => {
    vi.stubGlobal("localStorage", {
      getItem: vi.fn(() => null),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    });
    vi.mocked(getSessionHistory).mockResolvedValue({
      session_id: "source-session",
      authority_id: "personal-authority",
      turns: [{
        role: "assistant",
        content: "The source result arrived while this session was in the background.",
        content_digest: "result-digest",
        entry_id: "source-result-entry",
        entry_seq: 2,
        timestamp: "2026-09-23T12:00:00Z",
        tool_names: [],
      }],
      next_cursor: null,
    } satisfies SessionHistoryResponse);

    const cached = emptySessionRuntime("source-session");
    cached.messages = [{
      id: "source-session:user-entry",
      role: "user",
      content: "Please delegate this work.",
      timestamp: "2026-09-23T11:59:00Z",
      sessionId: "source-session",
      transcript: {
        authorityId: "personal-authority",
        sessionId: "source-session",
        entryId: "user-entry",
        entrySeq: 1,
      },
    } as ChatMessage];
    const hostState = {
      workshopEpoch: 0,
      workshopScopeId: "personal",
      sessionId: "other-session",
      transcriptEpoch: 0,
      sessionRuntimes: new Map([["source-session", cached]]),
      messages: [] as ChatMessage[],
      flushDraftPersist: vi.fn(),
      stashFocusedRuntime: vi.fn(),
      loadRuntimeIntoFocused: (_runtime: ReturnType<typeof emptySessionRuntime>) => {},
      tryReattachActiveTurn: vi.fn(async () => false),
      sanitizeTranscript: vi.fn(),
      noteResumeFailure: vi.fn(),
    };
    hostState.loadRuntimeIntoFocused = (runtime) => {
      hostState.sessionId = runtime.sessionId;
      hostState.messages = runtime.messages;
      hostState.transcriptEpoch = runtime.transcriptEpoch;
    };
    const host = hostState as unknown as ChatStoreHost;

    await switchSession(host, "source-session");
    await vi.waitFor(() => expect(getSessionHistory).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(host.messages.map((message) => message.content)).toContain(
      "The source result arrived while this session was in the background.",
    ));
    expect(host.sessionId).toBe("source-session");
    expect(host.tryReattachActiveTurn).toHaveBeenCalledOnce();
  });
});
