import { describe, expect, it } from "vitest";
import { LiveDelegationCoordinator } from "./liveDelegationCoordinator";
import type { LiveWorkResult } from "./liveWorkResult";

describe("Live work and speech overlap", () => {
  it("does not execute tools for a standalone acknowledgment", async () => {
    const coordinator = new LiveDelegationCoordinator();
    const signal = new AbortController().signal;
    let ran = false;
    expect(await coordinator.execute("Okay, no worries!", async () => {
      ran = true; return { status: "completed", text: "No" };
    }, signal)).toEqual({ kind: "acknowledgment" });
    expect(ran).toBe(false);
  });
  it("keeps the original result after okay no worries without starting another turn", async () => {
    const coordinator = new LiveDelegationCoordinator();
    const signal = new AbortController().signal;
    let finish!: (result: LiveWorkResult) => void;
    let calls = 0;
    const work = coordinator.execute("Check MCP tools", () => {
      calls += 1;
      return new Promise((resolve) => { finish = resolve; });
    }, signal);
    await Promise.resolve(); await Promise.resolve();
    expect(await coordinator.execute("Okay, no worries!", async () => {
      calls += 1;
      throw new Error("Must not run");
    }, signal)).toEqual({ kind: "acknowledgment" });
    expect(await coordinator.execute("", async () => {
      calls += 1; throw new Error("Must not run");
    }, signal)).toEqual({ kind: "acknowledgment" });
    finish({ status: "completed", text: "Six servers are connected." });
    expect(await work).toEqual({ kind: "result", result: { status: "completed", text: "Six servers are connected." } });
    expect(calls).toBe(1);
  });

  it("serializes real follow-up work rather than reporting an active-turn failure", async () => {
    const coordinator = new LiveDelegationCoordinator();
    const signal = new AbortController().signal;
    let release!: () => void;
    const order: string[] = [];
    const first = coordinator.execute("Search", async () => {
      order.push("first");
      await new Promise<void>((resolve) => { release = resolve; });
      return { status: "completed", text: "Found it" };
    }, signal);
    const second = coordinator.execute("Check GitHub too", async () => {
      order.push("second"); return { status: "completed", text: "Checked" };
    }, signal);
    await Promise.resolve(); await Promise.resolve();
    expect(order).toEqual(["first"]);
    release(); await first; await second;
    expect(order).toEqual(["first", "second"]);
  });

  it("does not start queued work after Live ends", async () => {
    const coordinator = new LiveDelegationCoordinator();
    const controller = new AbortController();
    controller.abort();
    let ran = false;
    await expect(coordinator.execute("Search", async () => {
      ran = true; return { status: "completed", text: "No" };
    }, controller.signal)).rejects.toThrow("before this request started");
    expect(ran).toBe(false);
  });
});
