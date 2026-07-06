import { describe, expect, it } from "vitest";

import {
  applyStreamSeq,
  CircuitBreaker,
  DEFAULT_INTERACTIVE_BACKOFF,
  OverlapGuard,
  reconnectDelayMs,
  streamPathWithSince,
} from "$lib/stream/reconnect";

describe("stream reconnect helpers", () => {
  it("appends since query param", () => {
    expect(streamPathWithSince("/v1/interactive/turn/t1/stream", 0)).toBe(
      "/v1/interactive/turn/t1/stream",
    );
    expect(streamPathWithSince("/v1/interactive/turn/t1/stream", 42)).toBe(
      "/v1/interactive/turn/t1/stream?since=42",
    );
    expect(streamPathWithSince("/v1/interactive/turn/t1/stream?since=1", 99)).toBe(
      "/v1/interactive/turn/t1/stream?since=99",
    );
  });

  it("dedupes stream seq", () => {
    const map = new Map<string, number>();
    expect(applyStreamSeq(map, { turn_id: "t1", seq: 2 })).toBe(true);
    expect(map.get("t1")).toBe(2);
    expect(applyStreamSeq(map, { turn_id: "t1", seq: 2 })).toBe(false);
    expect(applyStreamSeq(map, { turn_id: "t1", seq: 3 })).toBe(true);
  });

  it("caps backoff delay", () => {
    const delay = reconnectDelayMs(DEFAULT_INTERACTIVE_BACKOFF, 20);
    expect(delay).toBeLessThanOrEqual(DEFAULT_INTERACTIVE_BACKOFF.maxMs);
  });

  it("overlap guard admits one runner", () => {
    const guard = new OverlapGuard();
    expect(guard.tryEnter()).toBe(true);
    expect(guard.tryEnter()).toBe(false);
    guard.release();
    expect(guard.tryEnter()).toBe(true);
  });

  it("circuit breaker trips after threshold", () => {
    const breaker = new CircuitBreaker({ failureThreshold: 3 });
    expect(breaker.allow()).toBe(true);
    breaker.onFailure();
    breaker.onFailure();
    breaker.onFailure();
    expect(breaker.open).toBe(true);
    breaker.onSuccess();
    expect(breaker.open).toBe(false);
  });
});
