import { describe, expect, it } from "vitest";
import { createTimerState, migrateTimerState, reconcileTimerState, transitionTimer } from "./timerState";

describe("timerState", () => {
  it("uses absolute deadlines and reconciles suspension", () => {
    const running = transitionTimer(createTimerState(60_000), "start", 1_000);
    expect(running.deadlineAt).toBe(new Date(61_000).toISOString());
    expect(reconcileTimerState(running, 31_000).remainingMs).toBe(30_000);
    expect(reconcileTimerState(running, 70_000).status).toBe("complete");
  });

  it("pauses, resumes, resets, and migrates legacy snapshots", () => {
    const running = transitionTimer(createTimerState(10_000), "start", 0);
    const paused = transitionTimer(running, "pause", 4_000);
    expect(paused.remainingMs).toBe(6_000);
    expect(transitionTimer(paused, "resume", 5_000).deadlineAt).toBe(new Date(11_000).toISOString());
    expect(transitionTimer(paused, "reset").status).toBe("idle");
    expect(migrateTimerState({ status: "paused", duration_ms: 20_000, remaining_ms: 5_000 }, 1_000).remainingMs).toBe(5_000);
  });
});
