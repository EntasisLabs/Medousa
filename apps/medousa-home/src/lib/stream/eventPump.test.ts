import { describe, expect, it, vi } from "vitest";

import { StreamEventPump, type StreamEventTarget } from "$lib/stream/eventPump";
import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";

function streamEvent(
  seq: number,
  event: TurnStreamEventV2,
  turnId = "turn-1",
): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: turnId,
    seq,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    event,
  };
}

function target(event: TurnStreamEnvelopeV2): StreamEventTarget {
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
  it("coalesces adjacent appends without reordering content and reasoning lanes", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    pump.enqueue(target(streamEvent(1, { type: "content_append", text: "hel" })), 0);
    pump.enqueue(target(streamEvent(2, { type: "content_append", text: "lo" })), 0);
    pump.enqueue(target(streamEvent(3, { type: "reasoning_append", text: "hmm" })), 0);

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event).toMatchObject({
      seq: 2,
      event: { type: "content_append", text: "hello" },
    });
    scheduler.run();
    expect(apply).toHaveBeenCalledTimes(2);
    expect(apply.mock.calls[1][0].event).toMatchObject({
      seq: 3,
      event: { type: "reasoning_append", text: "hmm" },
    });
  });

  it("flushes append content before a semantic boundary", () => {
    const scheduler = manualScheduler();
    const applied: TurnStreamEnvelopeV2[] = [];
    const pump = new StreamEventPump(({ event }) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, { type: "content_append", text: "answer" })), 0);
    pump.enqueue(
      target(
        streamEvent(2, {
          type: "tool_started",
          tool_run_id: "run-1",
          tool_name: "search",
          input_summary: "query",
          tool_round: 1,
        }),
      ),
      0,
    );

    expect(applied.map((event) => event.event.type)).toEqual([
      "content_append",
      "tool_started",
    ]);
    scheduler.run();
    expect(applied).toHaveLength(2);
  });

  it("drops replay duplicates before they can contaminate a new batch", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    pump.enqueue(target(streamEvent(10, { type: "content_append", text: "duplicate" })), 10);
    pump.enqueue(target(streamEvent(11, { type: "content_append", text: "new" })), 10);
    pump.enqueue(target(streamEvent(11, { type: "content_append", text: "duplicate-new" })), 10);
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.event.text).toBe("new");
  });

  it("keeps independent turns in independent frame batches", () => {
    const scheduler = manualScheduler();
    const applied: StreamEventTarget[] = [];
    const pump = new StreamEventPump((event) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, { type: "content_append", text: "one" })), 0);
    pump.enqueue(target(streamEvent(1, { type: "content_append", text: "two" }, "turn-2")), 0);
    scheduler.run();

    expect(
      applied.map(({ event }) => [
        event.turn_id,
        event.event.type === "content_append" ? event.event.text : null,
      ]),
    ).toEqual([
      ["turn-1", "one"],
      ["turn-2", "two"],
    ]);
  });

  it("reduces a 10k-fragment hot path to one transcript apply", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    for (let seq = 1; seq <= 10_000; seq += 1) {
      pump.enqueue(target(streamEvent(seq, { type: "content_append", text: "x" })), 0);
    }
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.seq).toBe(10_000);
    expect(apply.mock.calls[0][0].event.event.text).toHaveLength(10_000);
  });
});
