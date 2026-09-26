import { afterEach, describe, expect, it, vi } from "vitest";

import {
  applyStreamSeq,
  CircuitBreaker,
  DEFAULT_INTERACTIVE_BACKOFF,
  OverlapGuard,
  reconnectDelayMs,
  ReconnectScheduler,
  streamPathWithSince,
} from "$lib/stream/reconnect";

describe("stream reconnect helpers", () => {
  afterEach(() => vi.useRealTimers());

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

  it("circuit breaker half-opens after cooldown and can self-recover", () => {
    const breaker = new CircuitBreaker({ failureThreshold: 3, cooldownMs: 10_000 });
    const t0 = 1_000_000;
    breaker.onFailure(t0);
    breaker.onFailure(t0);
    breaker.onFailure(t0);
    expect(breaker.currentState).toBe("open");

    // Still open before cooldown elapses.
    expect(breaker.allow(t0 + 5_000)).toBe(false);
    expect(breaker.remainingCooldownMs(t0 + 5_000)).toBe(5_000);

    // Cooldown elapsed -> a single half-open probe is admitted.
    expect(breaker.allow(t0 + 10_000)).toBe(true);
    expect(breaker.currentState).toBe("half_open");

    // A success fully closes the breaker again.
    breaker.onSuccess();
    expect(breaker.currentState).toBe("closed");
  });

  it("circuit breaker re-opens on half-open probe failure", () => {
    const breaker = new CircuitBreaker({ failureThreshold: 1, cooldownMs: 5_000 });
    const t0 = 2_000_000;
    breaker.onFailure(t0);
    expect(breaker.currentState).toBe("open");

    const t1 = t0 + 5_000;
    expect(breaker.allow(t1)).toBe(true);
    expect(breaker.currentState).toBe("half_open");

    // Probe failed -> back to open with a re-armed cooldown from t1.
    breaker.onFailure(t1);
    expect(breaker.currentState).toBe("open");
    expect(breaker.allow(t1 + 4_000)).toBe(false);
    expect(breaker.allow(t1 + 5_000)).toBe(true);
  });

  it("circuit breaker reset clears a tripped breaker", () => {
    const breaker = new CircuitBreaker({ failureThreshold: 1 });
    breaker.onFailure();
    expect(breaker.open).toBe(true);
    breaker.reset();
    expect(breaker.open).toBe(false);
    expect(breaker.allow()).toBe(true);
  });

  it("retries a failed live reattach after the current attempt releases its guard", async () => {
    vi.useFakeTimers();
    const scheduler = new ReconnectScheduler({
      policy: { baseMs: 1, factor: 1, maxMs: 1, maxAttempts: null },
    });
    let attempts = 0;
    const reattach = async () => {
      attempts += 1;
      if (attempts === 1) scheduler.schedule(reattach);
      else scheduler.teardown();
    };

    scheduler.schedule(reattach);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempts).toBe(1);
    expect(scheduler.pending).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempts).toBe(2);
  });

  it("coalesces overlapping retry requests and never starts concurrent attempts", async () => {
    vi.useFakeTimers();
    const scheduler = new ReconnectScheduler({
      policy: { baseMs: 1, factor: 1, maxMs: 1, maxAttempts: null },
    });
    let concurrent = 0;
    let maximumConcurrent = 0;
    let attempts = 0;
    let finishFirst: (() => void) | undefined;
    const attempt = async () => {
      attempts += 1;
      concurrent += 1;
      maximumConcurrent = Math.max(maximumConcurrent, concurrent);
      if (attempts === 1) {
        scheduler.schedule(attempt);
        scheduler.schedule(attempt);
        await new Promise<void>((resolve) => { finishFirst = resolve; });
      }
      concurrent -= 1;
      if (attempts === 2) scheduler.teardown();
    };

    scheduler.schedule(attempt);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempts).toBe(1);
    finishFirst?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(1);
    expect(attempts).toBe(2);
    expect(maximumConcurrent).toBe(1);
  });

  it("cancelling an in-flight recovery prevents it from rearming in the next scope", async () => {
    vi.useFakeTimers();
    const scheduler = new ReconnectScheduler({
      policy: { baseMs: 1, factor: 1, maxMs: 1, maxAttempts: null },
    });
    let attempts = 0;
    let finish: (() => void) | undefined;
    const attempt = async () => {
      attempts += 1;
      scheduler.schedule(attempt);
      await new Promise<void>((resolve) => { finish = resolve; });
    };

    scheduler.schedule(attempt);
    await vi.advanceTimersByTimeAsync(1);
    expect(attempts).toBe(1);
    scheduler.cancel();
    finish?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(100);
    expect(attempts).toBe(1);
  });

  it("keeps a fresh recovery requested after cancel until the old attempt settles", async () => {
    vi.useFakeTimers();
    const scheduler = new ReconnectScheduler({
      policy: { baseMs: 1, factor: 1, maxMs: 1, maxAttempts: null },
    });
    let finish: (() => void) | undefined;
    const fresh = vi.fn();
    scheduler.schedule(() => new Promise<void>((resolve) => { finish = resolve; }));
    await vi.advanceTimersByTimeAsync(1);
    scheduler.cancel();
    scheduler.schedule(fresh);
    await vi.advanceTimersByTimeAsync(100);
    expect(fresh).not.toHaveBeenCalled();
    finish?.();
    await vi.advanceTimersByTimeAsync(1);
    expect(fresh).toHaveBeenCalledTimes(1);
    scheduler.teardown();
  });
});
