import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ForgeProjectEvent } from "$lib/forge";

const mocks = vi.hoisted(() => ({ open: vi.fn(), runtime: "workshop-a" }));
vi.mock("$lib/daemon/daemonEventStream", () => ({ openDaemonEventStream: mocks.open }));
vi.mock("$lib/executionAuthority", () => ({ getCoderExecutionTransport: () => mocks.runtime }));
vi.mock("$lib/forge", () => ({ forgeProjectEventsUrl: vi.fn() }));
import { CodeProjectEventStream, subscribeCodeProjectEvents } from "./codeProjectEvents";

function connection() {
  return { closed: false, close: vi.fn(function (this: { closed: boolean }) { this.closed = true; }) };
}
function event(seq: number, kind = "changed", work_id = "work-a") {
  return { seq, kind, work_id, path: "src/main.rs", updated_at: "2026-09-08T00:00:00Z" } as ForgeProjectEvent;
}
const cleanup: Array<() => void> = [];

describe("shared project changes lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.runtime = "workshop-a";
    mocks.open.mockReset().mockImplementation(async () => connection());
  });
  afterEach(() => {
    for (const stop of cleanup.splice(0)) stop();
    vi.useRealTimers();
  });

  it("shares a connection across surfaces until the last subscriber leaves", async () => {
    const chat = vi.fn();
    const mobile = vi.fn();
    const stopChat = subscribeCodeProjectEvents("work-a", { onEvent: chat });
    const stopMobile = subscribeCodeProjectEvents("work-a", { onEvent: mobile });
    cleanup.push(stopMobile);
    await Promise.resolve();
    expect(mocks.open).toHaveBeenCalledTimes(1);
    const options = mocks.open.mock.calls[0][0];
    options.onEvent(event(1));
    expect(chat).toHaveBeenCalledTimes(1);
    expect(mobile).toHaveBeenCalledTimes(1);
    const source = await mocks.open.mock.results[0].value;
    stopChat();
    expect(source.close).not.toHaveBeenCalled();
    options.onEvent(event(2));
    expect(chat).toHaveBeenCalledTimes(1);
    expect(mobile).toHaveBeenCalledTimes(2);
    stopMobile();
    expect(source.close).toHaveBeenCalledTimes(1);
  });

  it("isolates identical project IDs on different workshops", async () => {
    cleanup.push(subscribeCodeProjectEvents("work-a", { onEvent: vi.fn() }));
    mocks.runtime = "workshop-b";
    cleanup.push(subscribeCodeProjectEvents("work-a", { onEvent: vi.fn() }));
    await Promise.resolve();
    expect(mocks.open.mock.calls.map(([options]) => options.executionRuntimeId))
      .toEqual(["workshop-a", "workshop-b"]);
  });

  it("polls while unavailable, resyncs on reconnect, and accepts a reset cursor", async () => {
    const onResync = vi.fn();
    const onEvent = vi.fn();
    cleanup.push(subscribeCodeProjectEvents("work-a", { onEvent, onResync }));
    await Promise.resolve();
    const first = mocks.open.mock.calls[0][0];
    first.onOpen();
    first.onEvent(event(80));
    first.onEvent(event(80));
    expect(onEvent).toHaveBeenCalledTimes(1);
    first.onError();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(onResync).toHaveBeenCalledTimes(2);
    const second = mocks.open.mock.calls[1][0];
    expect(second.query).toEqual({ since: "80" });
    second.onOpen();
    second.onEvent(event(0, "snapshot"));
    second.onEvent(event(1));
    expect(onEvent).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(onResync).toHaveBeenCalledTimes(3);
  });

  it("closes late connections and ignores callbacks after switching projects", async () => {
    let resolveFirst!: (source: ReturnType<typeof connection>) => void;
    mocks.open.mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve; }));
    const onEvent = vi.fn();
    const onResync = vi.fn();
    const stream = new CodeProjectEventStream({ onEvent, onResync });
    cleanup.push(() => stream.teardown());
    stream.setWorkId("work-a");
    const stale = mocks.open.mock.calls[0][0];
    stream.setWorkId("work-b");
    const source = connection();
    resolveFirst(source);
    await Promise.resolve();
    expect(source.close).toHaveBeenCalledTimes(1);
    stale.onEvent(event(99));
    stale.onOpen();
    stale.onError();
    expect(onEvent).not.toHaveBeenCalled();
    expect(onResync).not.toHaveBeenCalled();
    expect(stream.cursor).toBe(0);
    await vi.advanceTimersByTimeAsync(20_000);
    expect(mocks.open).toHaveBeenCalledTimes(2);
  });
});
