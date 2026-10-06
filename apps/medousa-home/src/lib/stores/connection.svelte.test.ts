import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connection } from "./connection.svelte";

beforeEach(() => { vi.useFakeTimers(); connection.setHealth(null); });
afterEach(() => { connection.setHealth(null); vi.useRealTimers(); });

describe("connection liveness", () => {
  it("real traffic restores connectivity without another health request", () => {
    connection.setHealth({ ok: false, message: "Probe failed" });
    expect(connection.offline).toBe(true);
    connection.noteTraffic();
    expect(connection.online).toBe(true);
    expect(connection.offline).toBe(false);
    expect(connection.checking).toBe(false);
  });

  it("a late failed probe cannot overwrite newer stream traffic", () => {
    const revision = connection.trafficRevision;
    connection.noteTraffic();
    connection.setHealth({ ok: false, message: "Older timeout" }, revision);
    expect(connection.online).toBe(true);
    connection.setHealth({ ok: false, message: "New failure" });
    expect(connection.online).toBe(true);
    vi.advanceTimersByTime(75_000);
    expect(connection.offline).toBe(true);
  });

  it("a heartbeat renews liveness without polling health", () => {
    connection.setHealth({ ok: false, message: "Probe failed" });
    connection.noteTraffic();
    vi.advanceTimersByTime(60_000);
    connection.noteTraffic();
    vi.advanceTimersByTime(60_000);
    expect(connection.online).toBe(true);
    vi.advanceTimersByTime(15_000);
    expect(connection.offline).toBe(true);
  });

  it("changing workshop clears the previous workshop's traffic evidence", () => {
    connection.noteTraffic();
    connection.setHealth(null);
    expect(connection.online).toBe(false);
    expect(connection.checking).toBe(true);
  });
});
