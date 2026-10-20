/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import { readChatPopoutRequest, writeChatPopoutRequest, clearChatPopoutRequest } from "./chatPopout";
beforeEach(() => { localStorage.clear(); vi.restoreAllMocks(); });
it("hands off only the exact session in the matching workshop", () => {
  writeChatPopoutRequest("labbing", "a");
  expect(readChatPopoutRequest("a")?.sessionId).toBe("labbing");
  expect(readChatPopoutRequest("b")).toBeNull();
});
it("does not reopen old requests or consume a newer click", () => {
  vi.spyOn(Date, "now").mockReturnValue(1000);
  const first = writeChatPopoutRequest("first", "a");
  vi.mocked(Date.now).mockReturnValue(1001);
  const second = writeChatPopoutRequest("second", "a");
  clearChatPopoutRequest(first);
  expect(readChatPopoutRequest("a")).toEqual(second);
  vi.mocked(Date.now).mockReturnValue(122000);
  expect(readChatPopoutRequest("a")).toBeNull();
});
