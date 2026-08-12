import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/daemon", () => ({
  cancelActiveSessionTurn: vi.fn(),
  getActiveSessionTurn: vi.fn(async () => ({ active: false, turn: null })),
  getSessionHistory: vi.fn(async () => ({ turns: [] })),
  listSessionTurns: vi.fn(async () => ({ turns: [] })),
  listSessions: vi.fn(async () => ({ sessions: [] })),
  deleteSession: vi.fn(),
  setSessionDisplayName: vi.fn(),
  startInteractiveStream: vi.fn(),
  stopInteractiveStreamTurn: vi.fn(async () => undefined),
}));

vi.mock("$lib/stores/shellTabs.svelte", () => ({
  shellTabs: {
    activeTab: null,
    openChat: vi.fn(),
  },
}));

vi.mock("$lib/stores/workshops.svelte", () => ({
  workshops: {
    saveActiveSession: vi.fn(async () => undefined),
  },
}));

vi.mock("$lib/liquid/surfaces/chat/chatScenes.svelte", () => ({
  chatScenes: { reset: vi.fn() },
}));

vi.mock("$lib/liquid/surfaces/chat/chatInteractions", () => ({
  chatInteractions: { reset: vi.fn() },
}));

vi.mock("$lib/stores/chatStreamPool.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./chatStreamPool.svelte")>();
  return {
    ...actual,
    chatStreamPool: new actual.ChatStreamPool(),
  };
});

describe("orphaned interactive turn lease", () => {
  beforeEach(() => {
    vi.resetModules();
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
      clear: () => storage.clear(),
    });
  });

  async function loadStore() {
    const { ChatStore } = await import("./chat.svelte");
    const { chatStreamPool } = await import("./chatStreamPool.svelte");
    const { getSessionHistory, listSessionTurns, getActiveSessionTurn } =
      await import("$lib/daemon");
    chatStreamPool.clear();
    chatStreamPool.setMaxLive(1);
    vi.mocked(getSessionHistory).mockResolvedValue({ turns: [] } as never);
    vi.mocked(listSessionTurns).mockResolvedValue({ turns: [] } as never);
    vi.mocked(getActiveSessionTurn).mockResolvedValue({
      active: false,
      turn: null,
    } as never);
    return { store: new ChatStore() };
  }

  it("releases stale interactive lease when daemon has no active turn", async () => {
    const { store } = await loadStore();
    store.beginTurn("hello", {
      turn_id: "turn-orphan",
      session_id: store.sessionId,
      mode: "interactive",
      phase: "streaming",
      accepted_at_utc: new Date().toISOString(),
      stream_url: "interactive://stream/turn-orphan",
      stream_ready: true,
      workspace_card_id: null,
    });

    expect(store.hasLiveInteractiveTurn()).toBe(true);
    expect(store.liveStreamActive).toBe(true);

    const attached = await store.tryReattachActiveTurn();
    expect(attached).toBe(false);
    expect(store.hasLiveInteractiveTurn()).toBe(false);
    expect(store.liveStreamActive).toBe(false);
    expect(store.messages.some((message) => message.streaming)).toBe(false);
  });

  it("merges history on resume after orphan clear instead of staying live", async () => {
    const { store } = await loadStore();
    const { getSessionHistory } = await import("$lib/daemon");
    store.beginTurn("hello", {
      turn_id: "turn-orphan-2",
      session_id: store.sessionId,
      mode: "interactive",
      phase: "streaming",
      accepted_at_utc: new Date().toISOString(),
      stream_url: "interactive://stream/turn-orphan-2",
      stream_ready: true,
      workspace_card_id: null,
    });

    vi.mocked(getSessionHistory).mockResolvedValue({
      turns: [
        {
          turn_id: "turn-orphan-2",
          role: "user",
          content: "hello",
          created_at: new Date().toISOString(),
        },
        {
          turn_id: "turn-orphan-2",
          role: "assistant",
          content: "done on the daemon",
          created_at: new Date().toISOString(),
        },
      ],
    } as never);

    await store.reconcileOnResume({ notice: false });
    expect(store.hasLiveInteractiveTurn()).toBe(false);
    expect(
      store.messages.some(
        (message) =>
          message.role === "assistant" && message.content.includes("done on the daemon"),
      ),
    ).toBe(true);
  });

  it("keeps the assistant bubble live across a recoverable stream failure", async () => {
    const { store } = await loadStore();
    store.beginTurn("hello", {
      turn_id: "turn-recoverable",
      session_id: store.sessionId,
      mode: "interactive",
      phase: "streaming",
      accepted_at_utc: new Date().toISOString(),
      stream_url: "interactive://stream/turn-recoverable",
      stream_ready: true,
      workspace_card_id: null,
    });

    store.noteStreamFailure("read HTTP response", { recoverable: true });

    const assistant = store.messages.find((message) => message.role === "assistant");
    expect(assistant?.failed).not.toBe(true);
    expect(assistant?.streaming).toBe(true);
    expect(store.hasLiveInteractiveTurn()).toBe(true);
  });

  it("fails and settles the assistant bubble for a non-recoverable stream error", async () => {
    const { store } = await loadStore();
    store.beginTurn("hello", {
      turn_id: "turn-non-recoverable",
      session_id: store.sessionId,
      mode: "interactive",
      phase: "streaming",
      accepted_at_utc: new Date().toISOString(),
      stream_url: "interactive://stream/turn-non-recoverable",
      stream_ready: true,
      workspace_card_id: null,
    });

    store.noteStreamFailure("invalid SSE JSON", { recoverable: false });

    const assistant = store.messages.find((message) => message.role === "assistant");
    expect(assistant?.failed).toBe(true);
    expect(assistant?.streaming).toBe(false);
    expect(store.hasLiveInteractiveTurn()).toBe(false);
  });
});
