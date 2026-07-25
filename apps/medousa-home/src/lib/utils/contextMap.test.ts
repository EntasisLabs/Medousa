import { describe, expect, it } from "vitest";
import type { LocusNodeSummary } from "$lib/types/locus";
import {
  applyPinnedPositions,
  buildContextMapGraph,
} from "$lib/utils/contextMap";

function moment(
  sessionId: string,
  syncKey: string,
  timestamp: string,
): LocusNodeSummary {
  return {
    sync_key: syncKey,
    session_id: sessionId,
    timestamp,
    tier: "raw",
    context_summary: syncKey,
    psi: 0.5,
    rho: 0.5,
    kappa: 0.4,
  };
}

describe("buildContextMapGraph", () => {
  it("links sessions by time proximity instead of a single chain", () => {
    const nodes = [
      moment("a", "a1", "2026-07-20T12:00:00Z"),
      moment("b", "b1", "2026-07-20T14:00:00Z"),
      moment("c", "c1", "2026-07-21T10:00:00Z"),
      moment("old", "o1", "2026-01-01T00:00:00Z"),
    ];

    const graph = buildContextMapGraph(nodes, {}, {
      width: 800,
      height: 600,
      expandedSessionIds: new Set(),
    });

    const sessionEdges = graph.edges.filter((edge) => edge.kind === "session_chain");
    expect(sessionEdges.length).toBeGreaterThan(1);

    const linked = new Set(
      sessionEdges.flatMap((edge) => [edge.from, edge.to]),
    );
    expect(linked.has("session:a")).toBe(true);
    expect(linked.has("session:b")).toBe(true);
    expect(linked.has("session:c")).toBe(true);
  });

  it("uses tighter layout floors in rail density", () => {
    const nodes = [moment("a", "a1", "2026-07-20T12:00:00Z")];
    const rail = buildContextMapGraph(nodes, {}, {
      width: 200,
      height: 200,
      expandedSessionIds: new Set(),
      density: "rail",
    });
    const full = buildContextMapGraph(nodes, {}, {
      width: 200,
      height: 200,
      expandedSessionIds: new Set(),
      density: "default",
    });
    expect(rail.width).toBeLessThan(full.width);
    expect(rail.height).toBeLessThan(full.height);
  });

  it("re-applies pinned positions after layout", () => {
    const nodes = [moment("a", "a1", "2026-07-20T12:00:00Z")];
    const graph = buildContextMapGraph(nodes, {}, {
      width: 800,
      height: 600,
      expandedSessionIds: new Set(),
    });
    const pinned = applyPinnedPositions(
      graph,
      new Map([["session:a", { x: 42, y: 77 }]]),
    );
    const session = pinned.nodes.find((node) => node.id === "session:a");
    expect(session?.x).toBe(42);
    expect(session?.y).toBe(77);
  });
});
