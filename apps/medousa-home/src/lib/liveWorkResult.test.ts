import { afterEach, describe, expect, it, vi } from "vitest";
import { liveWorkSnapshot, settledLiveWorkSnapshot, liveWorkResultEvents, waitForLiveWork, type LiveWorkSnapshot } from "./liveWorkResult";

afterEach(() => vi.useRealTimers());

describe("Live work results", () => {
  it("returns daemon-confirmed completion after the active turn disappears", async () => {
    expect(liveWorkSnapshot(undefined, [])).toBeNull();
    expect(settledLiveWorkSnapshot({ phase: "done" }, { content: "partial", streaming: true })).toBeNull();
    expect(settledLiveWorkSnapshot({ phase: "streaming" }, { content: "partial", streaming: false })).toBeNull();
    const result = await waitForLiveWork("settled", async () => settledLiveWorkSnapshot(
      { phase: "done" }, { streaming: false, content: "Six MCP servers are available." },
    ), new AbortController().signal);
    expect(result.status).toBe("completed");
    expect(result.text).toContain("Six MCP");
  });
  it("selects the original turn's assistant message, not the newest message", () => {
    expect(liveWorkSnapshot({ terminal: true, phase: "done", messageId: "original" }, [
      { id: "original", role: "assistant", content: "Owned answer" },
      { id: "new-chat", role: "assistant", content: "Unrelated answer" },
    ])?.text).toBe("Owned answer");
    expect(liveWorkSnapshot(undefined, [])).toBeNull();
  });
  it("waits for terminal completion rather than speaking partial text", async () => {
    vi.useFakeTimers();
    let state: LiveWorkSnapshot = { terminal: false, phase: "streaming", text: "partial" };
    const result = waitForLiveWork("turn-1", () => state, new AbortController().signal);
    state = { terminal: true, phase: "done", text: "The search found three sources." };
    await vi.advanceTimersByTimeAsync(250);
    expect(await result).toEqual({ status: "completed", turnId: "turn-1", text: state.text });
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each([ ["error", "failed"], ["cancelled", "cancelled"] ])("preserves %s outcomes", async (phase, status) => {
    expect(await waitForLiveWork("turn", () => ({ terminal: true, phase, text: "Details" }), new AbortController().signal))
      .toEqual({ status, turnId: "turn", text: "Details" });
  });

  it("reports a missing or still-running turn as pending, not success", async () => {
    vi.useFakeTimers();
    const result = waitForLiveWork("turn", () => null, new AbortController().signal, 1000);
    await vi.advanceTimersByTimeAsync(1000);
    expect((await result).status).toBe("pending");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("detaches on Live end and cleans timers", async () => {
    vi.useFakeTimers();
    const controller = new AbortController();
    const result = waitForLiveWork("turn", () => null, controller.signal);
    const assertion = expect(result).rejects.toThrow("work remains in chat");
    controller.abort();
    await assertion;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("returns the exact call result before requesting narration with tools disabled", () => {
    const events = liveWorkResultEvents("call-1", { status: "completed", text: "Actual answer", turnId: "turn-1" });
    expect(events[0].item?.call_id).toBe("call-1");
    expect(JSON.parse(events[0].item!.output)).toEqual({ status: "completed", text: "Actual answer", turnId: "turn-1" });
    expect(events[1].type).toBe("response.create");
    expect(events[1].response?.tool_choice).toBe("none");
  });
});
