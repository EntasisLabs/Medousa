/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSummary } from "$lib/types/session";
import type { ChatStoreHost } from "./chatStoreHost";
const mocks = vi.hoisted(() => ({ list: vi.fn() }));
vi.mock("$lib/daemon/session", async original => ({ ...await original<typeof import("$lib/daemon/session")>(), listSessions: mocks.list }));
import { mergeSessionSummaries, readMissingPins, readSessionPages, resetSessionCatalog } from "./sessionDiscovery";
import { loadMoreSessions, refreshSessions } from "./sessionController";

function session(id: string, name = id): SessionSummary {
  return { session_id: id, display_name: name, preview: "", turns: 313, verification_runs: 0 };
}
function host() {
  return { workshopEpoch: 0, sessions: [], pinnedIds: [], sessionsFetchedAt: 0,
    sessionsRefreshInFlight: null, sessionsRefreshDesiredQuery: null, sessionListQuery: "",
    sessionsRefreshing: false, sessionsError: null, sessionsCursor: null, sessionsBrowsePages: 1,
    sessionsListRevision: 0, sessionsLoadingOlder: false } as unknown as ChatStoreHost;
}
beforeEach(() => { mocks.list.mockReset(); });

describe("session discovery", () => {
  it("follows matching search pages, carrying q and the cursor", async () => {
    mocks.list.mockResolvedValueOnce({ sessions: [session("old", "labbing")], next_cursor: "next" })
      .mockResolvedValueOnce({ sessions: [session("old", "labbing updated"), session("older")], next_cursor: null });
    const result = await readSessionPages({ q: "labbing", current: () => true });
    expect(mocks.list).toHaveBeenNthCalledWith(2, { q: "labbing", cursor: "next", limit: 50, includeVerification: false });
    expect(result?.sessions.map(row => row.session_id)).toEqual(["old", "older"]);
    expect(result?.sessions[0].display_name).toBe("labbing updated");
  });
  it("detects repeated cursors instead of looping forever", async () => {
    mocks.list.mockResolvedValue({ sessions: [], next_cursor: "same" });
    await expect(readSessionPages({ current: () => true })).rejects.toThrow("repeated page");
    expect(mocks.list).toHaveBeenCalledTimes(2);
  });
  it("hydrates exact old pins independently and does not replace a pin with a substring hit", async () => {
    mocks.list.mockImplementation(async ({ q }) => ({ sessions: [session(`${q}-other`), ...(q === "old" ? [session("old")] : [])] }));
    const result = await readMissingPins(["recent", "old", "old", "gone"], [session("recent")], () => true);
    expect(result.pins.map(row => row.session_id)).toEqual(["old"]);
    expect(mocks.list).toHaveBeenCalledTimes(2);
    expect(result.failed).toBe(false);
  });
  it("keeps an available pin when a different lookup fails", async () => {
    mocks.list.mockImplementation(async ({ q }) => {
      if (q === "broken") throw new Error("offline");
      return { sessions: [session(q)] };
    });
    const result = await readMissingPins(["old", "broken"], [], () => true);
    expect(result.pins.map(row => row.session_id)).toEqual(["old"]);
    expect(result.failed).toBe(true);
  });
  it("deduplicates summaries with newer metadata winning", () => {
    expect(mergeSessionSummaries([session("a")], [session("a", "new")])).toEqual([session("a", "new")]);
  });
});

describe("browse cache pagination", () => {
  it("clears workshop-scoped catalog state and invalidates a pending page", async () => {
    const state = host();
    state.sessions = [session("old")]; state.pinnedIds = ["old"];
    state.sessionsCursor = "next"; state.sessionsBrowsePages = 3;
    state.sessionsFetchedAt = 123; state.sessionListQuery = "old";
    let finish!: (value: unknown) => void;
    mocks.list.mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const pending = loadMoreSessions(state);
    resetSessionCatalog(state);
    expect(state.sessionsLoadingOlder).toBe(false);
    expect(state.sessionsBrowsePages).toBe(1);
    expect(state.sessionsFetchedAt).toBe(0);
    expect(state.sessionListQuery).toBe("");
    expect(state.pinnedIds).toEqual([]);
    finish({ sessions: [session("stale")], next_cursor: "stale-next" });
    await pending;
    expect(state.sessions).toEqual([]);
    expect(state.sessionsCursor).toBeNull();
  });
  it("keeps browsing independent of search, hydrates old pins, and retains loaded depth on refresh", async () => {
    const state = host();
    state.pinnedIds = ["old-pin"];
    mocks.list.mockImplementation(async ({ cursor, q }) => {
      if (q) return { sessions: [session(q)] };
      return cursor ? { sessions: [session("older"), session("recent")], next_cursor: null }
        : { sessions: [session("recent")], next_cursor: "page-two" };
    });
    await refreshSessions(state, { force: true, q: "labbing" });
    expect(state.sessions.map(row => row.session_id)).toEqual(["recent", "old-pin"]);
    expect(state.sessionsCursor).toBe("page-two");
    await loadMoreSessions(state);
    expect(state.sessions.map(row => row.session_id)).toEqual(["recent", "old-pin", "older"]);
    expect(state.sessionsCursor).toBeNull();
    await refreshSessions(state, { force: true });
    expect(state.sessions.map(row => row.session_id)).toEqual(["recent", "older", "old-pin"]);
    expect(state.sessionsBrowsePages).toBe(2);
    expect(mocks.list.mock.calls.filter(([options]) => options.q === "labbing")).toHaveLength(0);
  });
  it("leaves a failed older page retryable without dropping already loaded chats", async () => {
    const state = host();
    state.sessions = [session("recent")]; state.sessionsCursor = "next";
    mocks.list.mockRejectedValueOnce(new Error("offline"));
    await loadMoreSessions(state);
    expect(state.sessionsCursor).toBe("next");
    expect(state.sessionsError).toBe("offline");
    expect(state.sessions.map(row => row.session_id)).toEqual(["recent"]);
    mocks.list.mockResolvedValueOnce({ sessions: [session("old")], next_cursor: null });
    await loadMoreSessions(state);
    expect(state.sessionsError).toBeNull();
    expect(state.sessions.map(row => row.session_id)).toEqual(["recent", "old"]);
  });
  it("ignores an older page returned after the workshop changed", async () => {
    const state = host(); state.sessionsCursor = "next";
    let finish!: (value: unknown) => void;
    mocks.list.mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const pending = loadMoreSessions(state);
    state.workshopEpoch += 1; state.sessions = [session("new-workshop")];
    state.sessionsLoadingOlder = false; state.sessionsCursor = null;
    finish({ sessions: [session("foreign")], next_cursor: "foreign-next" });
    await pending;
    expect(state.sessions.map(row => row.session_id)).toEqual(["new-workshop"]);
    expect(state.sessionsCursor).toBeNull();
  });
  it("preserves cached pins through lookup failures but drops confirmed missing ones", async () => {
    const state = host(); state.sessions = [session("cached-pin")]; state.pinnedIds = ["cached-pin"];
    mocks.list.mockImplementation(async ({ q }) => {
      if (q) throw new Error("offline");
      return { sessions: [session("recent")], next_cursor: null };
    });
    await refreshSessions(state, { force: true });
    expect(state.sessions.map(row => row.session_id)).toContain("cached-pin");
    expect(state.sessionsError).toContain("pinned chats");
    mocks.list.mockResolvedValue({ sessions: [], next_cursor: null });
    await refreshSessions(state, { force: true });
    expect(state.sessions.map(row => row.session_id)).not.toContain("cached-pin");
    expect(state.pinnedIds).toEqual(["cached-pin"]);
  });
  it("deduplicates simultaneous load-more clicks", async () => {
    const state = host(); state.sessionsCursor = "next";
    mocks.list.mockResolvedValue({ sessions: [], next_cursor: null });
    await Promise.all([loadMoreSessions(state), loadMoreSessions(state)]);
    expect(mocks.list).toHaveBeenCalledTimes(1);
  });
});
