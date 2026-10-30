import { expect, it } from "vitest";
import { browserSessionPage } from "./browserDaemon";
const rows = Array.from({ length: 121 }, (_, index) => ({ session_id: `session-${index}`, title: index === 120 ? "labbing" : `Recent ${index}`, updated_ms: 1000 - index }));
it("finds a browser-local chat beyond the first 50 before limiting", () => {
  expect(browserSessionPage(rows, 50, "LABBING").sessions.map(row => row.session_id)).toEqual(["session-120"]);
});
it("walks browser-local history without losing or duplicating older sessions", () => {
  const first = browserSessionPage(rows, 50);
  const second = browserSessionPage(rows, 50, "", first.next_cursor!);
  const third = browserSessionPage(rows, 50, "", second.next_cursor!);
  expect([first, second, third].map(page => page.sessions.length)).toEqual([50, 50, 21]);
  expect(new Set([...first.sessions, ...second.sessions, ...third.sessions].map(row => row.session_id)).size).toBe(121);
  expect(third.next_cursor).toBeNull();
});
it("pages filtered results and rejects a cursor from a different search", () => {
  const first = browserSessionPage(rows, 20, "Recent");
  const next = browserSessionPage(rows, 20, "recent", first.next_cursor!);
  expect(next.sessions[0].session_id).toBe("session-20");
  expect(() => browserSessionPage(rows, 20, "labbing", first.next_cursor!)).toThrow("Refresh");
});
it("searches preview and exact IDs, and handles an empty catalog", () => {
  expect(browserSessionPage([{ session_id: "older-id", title: "Other", preview: "labbing" }], 50, "labbing").sessions).toHaveLength(1);
  expect(browserSessionPage(rows, 50, "session-120").sessions[0].display_name).toBe("labbing");
  expect(browserSessionPage([], 50)).toEqual({ sessions: [], next_cursor: null });
});
