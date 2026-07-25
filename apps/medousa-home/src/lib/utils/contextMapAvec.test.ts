import { describe, expect, it } from "vitest";
import type { LocusNodeSummary } from "$lib/types/locus";
import {
  buildContextMapGraph,
  momentPassesAvecMins,
  type MapAvecMins,
} from "$lib/utils/contextMap";

function avec(
  partial: Partial<{
    stability: number;
    friction: number;
    logic: number;
    autonomy: number;
    psi: number;
  }> = {},
) {
  return {
    stability: partial.stability ?? 0.5,
    friction: partial.friction ?? 0.5,
    logic: partial.logic ?? 0.5,
    autonomy: partial.autonomy ?? 0.5,
    psi: partial.psi ?? 0.5,
  };
}

function locus(
  syncKey: string,
  sessionId: string,
  userAvec?: ReturnType<typeof avec> | null,
): LocusNodeSummary {
  return {
    sync_key: syncKey,
    session_id: sessionId,
    tier: "raw",
    timestamp: "2026-07-20T12:00:00Z",
    context_summary: syncKey,
    psi: 0.5,
    rho: 0.5,
    kappa: 0.5,
    user_avec: userAvec ?? undefined,
    model_avec: null,
  };
}

describe("momentPassesAvecMins", () => {
  const mins: MapAvecMins = {
    stability: 0.6,
    friction: 0,
    logic: 0,
    autonomy: 0,
  };

  it("passes all moments when mins are zero", () => {
    expect(momentPassesAvecMins(locus("a", "s1", null), {
      stability: 0,
      friction: 0,
      logic: 0,
      autonomy: 0,
    })).toBe(true);
  });

  it("hides moments missing user_avec when any min is active", () => {
    expect(momentPassesAvecMins(locus("a", "s1", null), mins)).toBe(false);
  });

  it("hides moments below a dimension threshold", () => {
    expect(
      momentPassesAvecMins(locus("a", "s1", avec({ stability: 0.4 })), mins),
    ).toBe(false);
  });

  it("keeps moments at or above every active threshold", () => {
    expect(
      momentPassesAvecMins(locus("a", "s1", avec({ stability: 0.6 })), mins),
    ).toBe(true);
  });
});

describe("buildContextMapGraph avecMins", () => {
  it("filters thread nodes by AVEC mins and drops empty sessions", () => {
    const nodes = [
      locus("low", "sess-a", avec({ stability: 0.2 })),
      locus("high", "sess-a", avec({ stability: 0.9 })),
      locus("other", "sess-b", avec({ stability: 0.1 })),
    ];
    const graph = buildContextMapGraph(nodes, { "sess-a": "Alpha", "sess-b": "Beta" }, {
      width: 800,
      height: 600,
      expandedSessionIds: new Set(["sess-a", "sess-b"]),
      avecMins: { stability: 0.5, friction: 0, logic: 0, autonomy: 0 },
    });

    const threadIds = graph.nodes
      .filter((node) => node.kind === "thread")
      .map((node) => node.syncKey);
    expect(threadIds).toEqual(["high"]);
    expect(graph.nodes.some((node) => node.id === "session:sess-b")).toBe(false);
    expect(graph.nodes.some((node) => node.id === "session:sess-a")).toBe(true);
  });
});
