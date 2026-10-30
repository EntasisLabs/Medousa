import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { SessionSummary } from "$lib/types/session";
const mocks = vi.hoisted(() => ({ list: vi.fn() }));
vi.mock("$lib/daemon/session", () => ({ listSessions: mocks.list }));
import { SessionSearch } from "./sessionSearch.svelte";
function row(id: string): SessionSummary {
  return { session_id: id, display_name: id, preview: "", turns: 313, verification_runs: 0 };
}
beforeEach(() => { vi.useFakeTimers(); mocks.list.mockReset(); });
afterEach(() => vi.useRealTimers());

it("finds an older chat outside the recent cache and follows all matching pages", async () => {
  const search = new SessionSearch();
  mocks.list.mockResolvedValueOnce({ sessions: [row("labbing")], next_cursor: "next" })
    .mockResolvedValueOnce({ sessions: [row("labbing-two")], next_cursor: null });
  const pending = search.search("workshop:1", "labbing");
  expect(search.loading).toBe(true);
  expect(mocks.list).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(180); await pending;
  expect(search.resultsFor("workshop:1", "labbing", [row("recent")]).map(session => session.session_id))
    .toEqual(["recent", "labbing", "labbing-two"]);
  expect(search.resultsFor("other:2", "labbing", [])).toEqual([]);
  expect(search.resultsFor("workshop:1", "different", [])).toEqual([]);
  expect(search.loading).toBe(false);
});
it("debounces typing and keeps sidebar and Spotlight searches independent", async () => {
  const sidebar = new SessionSearch(), spotlight = new SessionSearch();
  mocks.list.mockImplementation(async ({ q }) => ({ sessions: [row(q)] }));
  const first = sidebar.search("w:1", "lab");
  const second = sidebar.search("w:1", "labbing");
  const third = spotlight.search("w:1", "another");
  await vi.advanceTimersByTimeAsync(180); await Promise.all([first, second, third]);
  expect(mocks.list).toHaveBeenCalledTimes(2);
  expect(sidebar.sessions[0].session_id).toBe("labbing");
  expect(spotlight.sessions[0].session_id).toBe("another");
});
it("rejects a stale response after clearing a query or switching workshop", async () => {
  const search = new SessionSearch();
  let finish!: (value: unknown) => void;
  mocks.list.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const old = search.search("old:1", "labbing");
  await vi.advanceTimersByTimeAsync(180);
  search.cancel();
  await search.search("new:2", "");
  finish({ sessions: [row("foreign")], next_cursor: "foreign-page" });
  await old;
  expect(search.sessions).toEqual([]); expect(search.loading).toBe(false);
  expect(mocks.list).toHaveBeenCalledTimes(1);
});
it("does not let a slower query overwrite the current query", async () => {
  const search = new SessionSearch();
  let finish!: (value: unknown) => void;
  mocks.list.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const old = search.search("w:1", "old"); await vi.advanceTimersByTimeAsync(180);
  mocks.list.mockResolvedValueOnce({ sessions: [row("new")] });
  const next = search.search("w:1", "new"); await vi.advanceTimersByTimeAsync(180); await next;
  finish({ sessions: [row("old")] }); await old;
  expect(search.sessions.map(session => session.session_id)).toEqual(["new"]);
});
it("reports incomplete search and can retry without replacing the browse cache", async () => {
  const search = new SessionSearch(); const cached = [row("recent")];
  mocks.list.mockRejectedValueOnce(new Error("offline"));
  const failed = search.search("w:1", "labbing"); await vi.advanceTimersByTimeAsync(180); await failed;
  expect(search.error).toBe("offline");
  expect(search.resultsFor("w:1", "labbing", cached)).toEqual(cached);
  mocks.list.mockResolvedValueOnce({ sessions: [row("labbing")] });
  const retry = search.search("w:1", "labbing"); await vi.advanceTimersByTimeAsync(180); await retry;
  expect(search.error).toBeNull(); expect(search.sessions).toHaveLength(1);
  expect(cached.map(session => session.session_id)).toEqual(["recent"]);
});
