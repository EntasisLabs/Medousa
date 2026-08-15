import { describe, expect, it, vi } from "vitest";

import { StreamEventPump, type StreamEventTarget } from "$lib/stream/eventPump";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

function streamEvent(
  seq: number,
  eventType: string,
  fields: Partial<InteractiveTurnStreamEvent> = {},
): InteractiveTurnStreamEvent {
  return {
    turn_id: "turn-1",
    seq,
    event_type: eventType,
    phase: "streaming",
    message: "",
    terminal: false,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    ...fields,
  };
}

function target(event: InteractiveTurnStreamEvent): StreamEventTarget {
  return { sessionId: "session-1", event };
}

function manualScheduler() {
  let flush: (() => void) | null = null;
  return {
    schedule: (callback: () => void) => {
      flush = callback;
      return () => {
        flush = null;
      };
    },
    run: () => {
      const callback = flush as (() => void) | null;
      flush = null;
      callback?.();
    },
  };
}

describe("StreamEventPump", () => {
  it("coalesces append storms into one scheduled apply", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    pump.enqueue(target(streamEvent(1, "content_delta", { content_delta: "hel" })), 0);
    pump.enqueue(target(streamEvent(2, "content_delta", { content_delta: "lo" })), 0);
    pump.enqueue(target(streamEvent(3, "reasoning_delta", { reasoning_delta: "hmm" })), 0);

    expect(apply).not.toHaveBeenCalled();
    scheduler.run();
    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event).toMatchObject({
      seq: 3,
      content_delta: "hello",
      reasoning_delta: "hmm",
    });
  });

  it("flushes append content before a semantic boundary", () => {
    const scheduler = manualScheduler();
    const applied: InteractiveTurnStreamEvent[] = [];
    const pump = new StreamEventPump(({ event }) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, "content_delta", { content_delta: "answer" })), 0);
    pump.enqueue(
      target(streamEvent(2, "tool_started", { tool_run_id: "run-1", tool_name: "search" })),
      0,
    );

    expect(applied.map((event) => event.event_type)).toEqual(["content_delta", "tool_started"]);
    scheduler.run();
    expect(applied).toHaveLength(2);
  });

  it("drops replay duplicates before they can contaminate a new batch", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    pump.enqueue(target(streamEvent(10, "content_delta", { content_delta: "duplicate" })), 10);
    pump.enqueue(target(streamEvent(11, "content_delta", { content_delta: "new" })), 10);
    pump.enqueue(target(streamEvent(11, "content_delta", { content_delta: "duplicate-new" })), 10);
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.content_delta).toBe("new");
  });

  it("keeps independent turns in independent frame batches", () => {
    const scheduler = manualScheduler();
    const applied: StreamEventTarget[] = [];
    const pump = new StreamEventPump((event) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, "content_delta", { content_delta: "one" })), 0);
    pump.enqueue(
      target({ ...streamEvent(1, "content_delta", { content_delta: "two" }), turn_id: "turn-2" }),
      0,
    );
    scheduler.run();

    expect(applied.map(({ event }) => [event.turn_id, event.content_delta])).toEqual([
      ["turn-1", "one"],
      ["turn-2", "two"],
    ]);
  });

  it("reduces a 10k-fragment hot path to one transcript apply", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    for (let seq = 1; seq <= 10_000; seq += 1) {
      pump.enqueue(target(streamEvent(seq, "content_delta", { content_delta: "x" })), 0);
    }
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.seq).toBe(10_000);
    expect(apply.mock.calls[0][0].event.content_delta).toHaveLength(10_000);
  });
});
