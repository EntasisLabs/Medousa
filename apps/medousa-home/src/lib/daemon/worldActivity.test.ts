import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ unary: vi.fn() }));

vi.mock("$lib/daemon/contractClient", () => ({
  daemonUnary: (...args: unknown[]) => mocks.unary(...args),
}));

import {
  loadLatestWorldActivity,
  mergeWorldActivity,
  worldActivityDetailLabel,
  worldActivityTone,
  worldActivityWhen,
  worldActorLabel,
} from "$lib/daemon/worldActivity";
import type {
  WorldEvidenceResponse,
  WorldTimelineEvent,
  WorldTimelineResponse,
} from "$lib/types/generated/daemon_api";

function event(sequence: number, overrides: Partial<WorldTimelineEvent> = {}): WorldTimelineEvent {
  return {
    authority_id: "authority:workshop",
    driver_id: "driver:browser",
    event_type: "action_committed",
    occurred_at_ms: 10,
    ownership: "owned",
    recorded_at_ms: 11,
    schema_version: 1,
    sequence,
    surface: "browser",
    world_id: "world:one",
    world_revision: 2,
    ...overrides,
  };
}

describe("world activity client", () => {
  beforeEach(() => mocks.unary.mockReset());

  it("loads one bounded tail page and binds evidence by ledger sequence", async () => {
    const timeline: WorldTimelineResponse = {
      events: [event(7), event(8, { principal_kind: "human" })],
      has_more: true,
      next_sequence: 8,
    };
    const evidence: WorldEvidenceResponse = {
      evidence: [
        {
          authority_id: "authority:workshop",
          driver_id: "driver:browser",
          event_type: "action_committed",
          evidence_id: "world-evidence:8",
          ledger_sequence: 8,
          ownership: "owned",
          promoted_at_ms: 11,
          reasons: ["sensitive_effect"],
          schema_version: 1,
          surface: "browser",
          world_id: "world:one",
        },
      ],
      has_more: false,
      next_sequence: 8,
    };
    mocks.unary.mockResolvedValueOnce(timeline).mockResolvedValueOnce(evidence);

    const page = await loadLatestWorldActivity("runtime:remote", 500);

    expect(page.items.map((item) => item.event.sequence)).toEqual([8, 7]);
    expect(page.items[0].evidence?.reasons).toEqual(["sensitive_effect"]);
    expect(page.hasEarlier).toBe(true);
    expect(mocks.unary.mock.calls).toEqual([
      ["worlds.timeline.get", {}, undefined, "runtime:remote", { tail: "true", limit: "200" }],
      ["worlds.evidence.get", {}, undefined, "runtime:remote", { tail: "true", limit: "200" }],
    ]);
  });

  it("keeps the causal timeline usable when an older daemon lacks evidence", async () => {
    mocks.unary.mockResolvedValueOnce({
      events: [event(2)],
      has_more: false,
      next_sequence: 2,
    });
    mocks.unary.mockRejectedValueOnce(new Error("HTTP 404"));

    const page = await loadLatestWorldActivity();

    expect(page.items).toHaveLength(1);
    expect(page.evidenceAvailable).toBe(false);
  });

  it("labels principals and risky terminal states without reading payloads", () => {
    const interrupted = event(3, {
      event_type: "action_interrupted",
      principal_kind: "agent",
      status: "indeterminate",
    });
    expect(worldActorLabel(interrupted)).toBe("Medousa");
    expect(worldActivityTone(interrupted)).toBe("danger");
    expect(worldActivityDetailLabel("needs_reconciliation")).toBe("Needs Reconciliation");
    expect(worldActivityWhen(1_000, 61_000)).toBe("1m");
    expect(
      mergeWorldActivity(
        { events: [interrupted], has_more: false, next_sequence: 3 },
        null,
      ).items[0].event.summary,
    ).toBeUndefined();
  });
});
