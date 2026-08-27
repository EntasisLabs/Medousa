import { describe, expect, it, vi } from "vitest";

import { StreamEventPump, type StreamEventTarget } from "$lib/stream/eventPump";
import type {
  TurnStreamEnvelopeV3,
  TurnStreamEventV3,
} from "$lib/types/generated/daemon_api";

function streamEvent(
  seq: number,
  event: TurnStreamEventV3,
  turnId = "turn-1",
): TurnStreamEnvelopeV3 {
  return {
    schema_version: 3,
    turn_id: turnId,
    seq,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    event,
  };
}

function target(event: TurnStreamEnvelopeV3): StreamEventTarget {
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

    pump.enqueue(target(streamEvent(1, { type: "content_append", segment_id: "s1", text: "hel" })), 0);
    pump.enqueue(target(streamEvent(2, { type: "content_append", segment_id: "s1", text: "lo" })), 0);
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
    const applied: TurnStreamEnvelopeV3[] = [];
    const pump = new StreamEventPump(({ event }) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, { type: "content_append", segment_id: "s1", text: "answer" })), 0);
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

  it("never coalesces adjacent appends from different text segments", () => {
    const scheduler = manualScheduler();
    const applied: TurnStreamEnvelopeV3[] = [];
    const pump = new StreamEventPump(({ event }) => applied.push(event), scheduler.schedule);

    pump.enqueue(
      target(streamEvent(1, { type: "content_append", segment_id: "s1", text: "before" })),
      0,
    );
    pump.enqueue(
      target(streamEvent(2, { type: "content_append", segment_id: "s2", text: "after" })),
      0,
    );
    scheduler.run();

    expect(applied.map(({ event }) => event)).toEqual([
      { type: "content_append", segment_id: "s1", text: "before" },
      { type: "content_append", segment_id: "s2", text: "after" },
    ]);
  });

  it("drops replay duplicates before they can contaminate a new batch", () => {
    const scheduler = manualScheduler();
    const apply = vi.fn();
    const pump = new StreamEventPump(apply, scheduler.schedule);

    pump.enqueue(target(streamEvent(10, { type: "content_append", segment_id: "s1", text: "duplicate" })), 10);
    pump.enqueue(target(streamEvent(11, { type: "content_append", segment_id: "s1", text: "new" })), 10);
    pump.enqueue(target(streamEvent(11, { type: "content_append", segment_id: "s1", text: "duplicate-new" })), 10);
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.event.text).toBe("new");
  });

  it("keeps independent turns in independent frame batches", () => {
    const scheduler = manualScheduler();
    const applied: StreamEventTarget[] = [];
    const pump = new StreamEventPump((event) => applied.push(event), scheduler.schedule);

    pump.enqueue(target(streamEvent(1, { type: "content_append", segment_id: "s1", text: "one" })), 0);
    pump.enqueue(target(streamEvent(1, { type: "content_append", segment_id: "s2", text: "two" }, "turn-2")), 0);
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
      pump.enqueue(target(streamEvent(seq, { type: "content_append", segment_id: "s1", text: "x" })), 0);
    }
    scheduler.run();

    expect(apply).toHaveBeenCalledTimes(1);
    expect(apply.mock.calls[0][0].event.seq).toBe(10_000);
    expect(apply.mock.calls[0][0].event.event.text).toHaveLength(10_000);
  });
});
