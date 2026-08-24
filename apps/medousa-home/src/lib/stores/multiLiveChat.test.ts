import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { MAX_SHELL_PANES } from "$lib/types/shellTabs";

vi.mock("$lib/daemon", () => ({
  cancelActiveSessionTurn: vi.fn(),
  deriveSession: vi.fn(),
  getActiveSessionTurn: vi.fn(async () => null),
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

vi.mock("$lib/chat/chatStreamPool.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/chat/chatStreamPool.svelte")>();
  return {
    ...actual,
    chatStreamPool: new actual.ChatStreamPool(),
  };
});

function msg(id: string, content: string, role: ChatMessage["role"] = "user"): ChatMessage {
  return { id, role, content };
}

describe("multi-live chat session runtimes", () => {
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
    const { chatStreamPool } = await import("$lib/chat/chatStreamPool.svelte");
    const { getSessionHistory } = await import("$lib/daemon");
    chatStreamPool.clear();
    chatStreamPool.setMaxLive(1);
    vi.mocked(getSessionHistory).mockResolvedValue({ turns: [] } as never);
    return { store: new ChatStore(), chatStreamPool };
  }

  it("bootstrapMultiLive sets maxLive to pane cap and acquires current session", async () => {
    const { store, chatStreamPool } = await loadStore();
    store.sessionId = "sess-focused";
    store.bootstrapMultiLive();
    expect(chatStreamPool.maxLiveStreams).toBe(MAX_SHELL_PANES);
    expect(chatStreamPool.isLive(store.sessionId)).toBe(true);
  });

  it("forks a committed prefix and carries the draft only into the target", async () => {
    const { store } = await loadStore();
    const { deriveSession, getSessionHistory } = await import("$lib/daemon");
    const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
    const committed: ChatMessage = {
      id: "source:assistant:2",
      role: "assistant",
      content: "Committed answer",
      transcript: {
        authorityId: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        sessionId: "source-session",
        entryId: "ent_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        entrySeq: 2,
      },
    };
    store.sessionId = "source-session";
    store.messages = [committed];
    store.draft = "follow this line of thought";
    store.sessionPristine = false;
    store.sessions = [
      {
        session_id: "source-session",
        display_name: "Source idea",
        turns: 2,
        verification_runs: 0,
        preview: "Committed answer",
      },
    ];

    vi.mocked(deriveSession).mockResolvedValue({
      authority_id: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      session_id: "target-session",
      catalog: "single",
      display_name: "Fork of Source idea",
      reused: false,
      derivation: {},
    } as never);
    vi.mocked(getSessionHistory).mockResolvedValue({
      authority_id: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      session_id: "target-session",
      turns: [],
    });

    await store.forkFromEntry(committed, { includeDraft: true });
    const { loadDraftForSession } = await import("$lib/chat/draftPersistence");

    expect(deriveSession).toHaveBeenCalledWith(
      {
        intent: "fork",
        sources: [
          {
            session: {
              authority_id: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              session_id: "source-session",
            },
            through_entry_seq: 2,
          },
        ],
        target: { catalog: "single", display_name: "Fork of Source idea" },
      },
      expect.stringMatching(/^home-fork-/),
    );
    expect(store.sessionId).toBe("target-session");
    expect(store.draft).toBe("follow this line of thought");
    expect(loadDraftForSession("source-session")).toBe("follow this line of thought");
    expect(loadDraftForSession("target-session")).toBe("follow this line of thought");
    expect(shellTabs.openChat).toHaveBeenCalledWith("target-session", {
      activate: true,
      title: "Fork of Source idea",
    });
  });

  it("maps committed transcript coordinates and derivation provenance into UI messages", async () => {
    const { mapTurns } = await import("$lib/chat/sessionController");
    const messages = mapTurns(
      [
        {
          entry_id: "ent_cccccccccccccccccccccccccccccccc",
          entry_seq: 4,
          content_digest: "v1:content",
          role: "assistant",
          content: "Derived answer",
          timestamp: "2026-08-21T12:00:00Z",
          tool_names: [],
          source: {
            session: {
              authority_id: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              session_id: "source-session",
            },
            entry_id: "ent_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            entry_seq: 4,
          },
        },
      ],
      {
        authorityId: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        sessionId: "target-session",
      },
    );

    expect(messages[0]?.transcript).toEqual({
      authorityId: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      sessionId: "target-session",
      entryId: "ent_cccccccccccccccccccccccccccccccc",
      entrySeq: 4,
      source: {
        authorityId: "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        sessionId: "source-session",
        entryId: "ent_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        entrySeq: 4,
      },
    });
  });

  it("bootstrapMultiLive acquires listed sessions and warms background history", async () => {
    const { store, chatStreamPool } = await loadStore();
    const { getSessionHistory } = await import("$lib/daemon");
    vi.mocked(getSessionHistory).mockResolvedValue({
      turns: [
        {
          role: "user",
          content: "hello background",
          timestamp: new Date().toISOString(),
        },
      ],
    } as never);

    store.sessionId = "sess-focused";
    store.bootstrapMultiLive(["sess-focused", "sess-bg"]);
    expect(chatStreamPool.isLive("sess-focused")).toBe(true);
    expect(chatStreamPool.isLive("sess-bg")).toBe(true);

    await vi.waitFor(() => {
      expect(store.messagesFor("sess-bg").length).toBeGreaterThan(0);
    });
  });

  it("keeps session A transcript in memory after focus swap to B and back", async () => {
    const { store } = await loadStore();
    store.sessionId = "sess-a";
    store.messages = [msg("a1", "from A")];
    store.draft = "draft-a";
    store.sessionPristine = false;

    await store.switchSession("sess-b");
    expect(store.sessionId).toBe("sess-b");
    expect(store.messagesFor("sess-a")).toEqual([msg("a1", "from A")]);
    expect(store.messages).toEqual([]);

    store.messages = [msg("b1", "from B")];
    await store.switchSession("sess-a");
    expect(store.sessionId).toBe("sess-a");
    expect(store.messages).toEqual([msg("a1", "from A")]);
    expect(store.messagesFor("sess-b")).toEqual([msg("b1", "from B")]);
    expect(store.draft).toBe("draft-a");
  });

  it("withSessionFields applies mutations to a non-focused runtime", async () => {
    const { store } = await loadStore();
    store.sessionId = "sess-a";
    store.messages = [msg("a1", "A")];
    (store as unknown as { stashFocusedRuntime: () => void }).stashFocusedRuntime();

    store.sessionId = "sess-b";
    store.messages = [msg("b1", "B")];
    (store as unknown as { stashFocusedRuntime: () => void }).stashFocusedRuntime();

    (store as unknown as { withSessionFields: (id: string, fn: () => void) => void }).withSessionFields(
      "sess-a",
      () => {
        store.messages = [...store.messages, msg("a2", "streamed")];
      },
    );

    expect(store.sessionId).toBe("sess-b");
    expect(store.messages).toEqual([msg("b1", "B")]);
    expect(store.messagesFor("sess-a")).toEqual([msg("a1", "A"), msg("a2", "streamed")]);
  });

  it("switchSession bumps transcriptEpoch so stale reconcile cannot append prior chat", async () => {
    const { store } = await loadStore();
    const { getSessionHistory } = await import("$lib/daemon");

    store.sessionId = "sess-a";
    store.messages = [msg("a1", "from A")];
    store.sessionPristine = false;
    const epochBefore = (store as unknown as { transcriptEpoch: number }).transcriptEpoch;

    type Resolver = (value: unknown) => void;
    const pending = new Map<string, Resolver[]>();
    vi.mocked(getSessionHistory).mockImplementation((sessionId: string) => {
      return new Promise((resolve) => {
        const list = pending.get(sessionId) ?? [];
        list.push(resolve as Resolver);
        pending.set(sessionId, list);
      }) as never;
    });

    const reconcilePromise = store.reconcileOnResume({ notice: false });
    await vi.waitFor(() => expect(pending.has("sess-a")).toBe(true));

    const switchPromise = store.switchSession("sess-b");
    await vi.waitFor(() =>
      expect((store as unknown as { transcriptEpoch: number }).transcriptEpoch).toBeGreaterThan(
        epochBefore,
      ),
    );
    await vi.waitFor(() => expect(pending.has("sess-b")).toBe(true));

    for (const resolve of pending.get("sess-a") ?? []) {
      resolve({
        turns: [
          {
            role: "user",
            content: "stale A history",
            timestamp: new Date().toISOString(),
          },
        ],
      });
    }
    for (const resolve of pending.get("sess-b") ?? []) {
      resolve({ turns: [] });
    }
    await reconcilePromise;
    await switchPromise;

    expect(store.sessionId).toBe("sess-b");
    expect(store.messages.map((m) => m.content)).not.toContain("stale A history");
    expect(store.messages.map((m) => m.content)).not.toContain("from A");
  });

  it("keeps focusedSessionId and focused transcript stable during background stream apply", async () => {
    const { store } = await loadStore();
    const { emptySessionRuntime } = await import("$lib/chat/chatSessionRuntime");
    type RuntimeMap = Map<string, ReturnType<typeof emptySessionRuntime>>;
    const runtimes = (store as unknown as { sessionRuntimes: RuntimeMap }).sessionRuntimes;

    store.sessionId = "sess-focus";
    store.messages = [msg("f1", "focused")];
    (store as unknown as { stashFocusedRuntime: () => void }).stashFocusedRuntime();

    const bg = emptySessionRuntime("sess-bg");
    bg.messages = [msg("b1", "background")];
    runtimes.set("sess-bg", bg);

    let sawFocusedDuringApply = "";
    let sawFocusedMessages: string[] = [];
    (store as unknown as { withSessionFields: (id: string, fn: () => void) => void }).withSessionFields(
      "sess-bg",
      () => {
        sawFocusedDuringApply = store.focusedSessionId;
        sawFocusedMessages = store.messagesFor("sess-focus").map((m) => m.content);
        store.messages = [...store.messages, msg("b2", "delta")];
      },
    );

    expect(sawFocusedDuringApply).toBe("sess-focus");
    expect(sawFocusedMessages).toEqual(["focused"]);
    expect(store.focusedSessionId).toBe("sess-focus");
    expect(store.messagesFor("sess-focus")).toEqual([msg("f1", "focused")]);
    expect(store.messagesFor("sess-bg")).toEqual([
      msg("b1", "background"),
      msg("b2", "delta"),
    ]);
  });

  it("keeps secure credential prompts in their owning chat session", async () => {
    const { store } = await loadStore();
    const { emptySessionRuntime } = await import("$lib/chat/chatSessionRuntime");
    type RuntimeMap = Map<string, ReturnType<typeof emptySessionRuntime>>;
    const runtimes = (store as unknown as { sessionRuntimes: RuntimeMap }).sessionRuntimes;

    store.sessionId = "sess-focus";
    store.secretAlert = null;
    (store as unknown as { stashFocusedRuntime: () => void }).stashFocusedRuntime();
    runtimes.set("sess-bg", emptySessionRuntime("sess-bg"));

    (store as unknown as { withSessionFields: (id: string, fn: () => void) => void }).withSessionFields(
      "sess-bg",
      () => {
        store.secretAlert = {
          turnId: "turn-bg",
          messageId: null,
          requestId: "asecret-bg",
          label: "GitHub token",
          reason: "Read a private repository",
          providerType: "github",
          credentialKey: "GITHUB_TOKEN",
          backend: "openshell_provider",
          allowedHosts: [],
        };
      },
    );

    expect(store.sessionId).toBe("sess-focus");
    expect(store.secretAlert).toBeNull();
    expect(runtimes.get("sess-bg")?.secretAlert?.requestId).toBe("asecret-bg");
  });
});
