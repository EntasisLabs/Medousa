import { describe, expect, it } from "vitest";
import { createContextMapSimulation } from "$lib/utils/contextMapPhysics";

describe("createContextMapSimulation", () => {
  it("pulls linked nodes toward the membership distance while settling", () => {
    const sim = createContextMapSimulation();
    sim.setTopology(
      [
        { id: "session:a", kind: "session", radius: 14, weight: 4, x: 0, y: 0 },
        { id: "thread:1", kind: "thread", radius: 6, weight: 1, x: 400, y: 0 },
      ],
      [
        {
          id: "membership:a:1",
          from: "session:a",
          to: "thread:1",
          kind: "membership",
          strength: 0.14,
        },
      ],
      800,
      600,
    );

    const before = sim.getPositions();
    const distBefore = Math.hypot(
      (before.get("thread:1")?.x ?? 0) - (before.get("session:a")?.x ?? 0),
      (before.get("thread:1")?.y ?? 0) - (before.get("session:a")?.y ?? 0),
    );

    for (let i = 0; i < 80; i += 1) sim.tick();

    const after = sim.getPositions();
    const distAfter = Math.hypot(
      (after.get("thread:1")?.x ?? 0) - (after.get("session:a")?.x ?? 0),
      (after.get("thread:1")?.y ?? 0) - (after.get("session:a")?.y ?? 0),
    );

    expect(distAfter).toBeLessThan(distBefore);
    sim.dispose();
  });

  it("keeps a pinned node fixed while neighbors move", () => {
    const sim = createContextMapSimulation();
    sim.setTopology(
      [
        { id: "session:a", kind: "session", radius: 14, weight: 4, x: 200, y: 200 },
        { id: "thread:1", kind: "thread", radius: 6, weight: 1, x: 360, y: 200 },
        { id: "thread:2", kind: "thread", radius: 6, weight: 1, x: 40, y: 200 },
      ],
      [
        {
          id: "m1",
          from: "session:a",
          to: "thread:1",
          kind: "membership",
        },
        {
          id: "m2",
          from: "session:a",
          to: "thread:2",
          kind: "membership",
        },
      ],
      800,
      600,
    );

    sim.pin("session:a", 220, 240);
    for (let i = 0; i < 40; i += 1) sim.tick();

    const pinned = sim.getPositions().get("session:a");
    expect(pinned?.x).toBeCloseTo(220, 5);
    expect(pinned?.y).toBeCloseTo(240, 5);

    const thread = sim.getPositions().get("thread:1");
    expect(thread).toBeTruthy();
    expect(thread!.x !== 360 || thread!.y !== 200).toBe(true);
    sim.dispose();
  });

  it("sleeps after alpha decays without pins", () => {
    const sim = createContextMapSimulation();
    sim.setTopology(
      [
        { id: "session:a", kind: "session", radius: 12, weight: 2, x: 100, y: 100 },
        { id: "session:b", kind: "session", radius: 12, weight: 2, x: 300, y: 120 },
      ],
      [
        {
          id: "chain",
          from: "session:a",
          to: "session:b",
          kind: "session_chain",
        },
      ],
      800,
      600,
    );

    let awake = true;
    let steps = 0;
    while (awake && steps < 400) {
      awake = sim.tick();
      steps += 1;
    }

    expect(awake).toBe(false);
    expect(sim.isSleeping()).toBe(true);
    expect(steps).toBeGreaterThan(10);
    sim.dispose();
  });

  it("wakes again after restart", () => {
    const sim = createContextMapSimulation();
    sim.setTopology(
      [{ id: "session:a", kind: "session", radius: 12, weight: 2, x: 100, y: 100 }],
      [],
      800,
      600,
    );
    while (sim.tick()) {
      /* cool */
    }
    expect(sim.isSleeping()).toBe(true);
    sim.restart({ alpha: 0.5 });
    expect(sim.isSleeping()).toBe(false);
    expect(sim.tick()).toBe(true);
    sim.dispose();
  });
});
