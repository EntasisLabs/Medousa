import { describe, expect, it } from "vitest";
import type { TurnStreamEnvelopeV2, TurnStreamEventV2 } from "@medousa/client";
import { createProjectionState, projectStreamEvent } from "../src/projection.js";

function envelope(event: TurnStreamEventV2): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    emitted_at_utc: "2026-08-02T00:00:00Z",
    turn_id: "turn-one",
    seq: 1,
    event,
  };
}

describe("browser stream projection", () => {
  it("keeps deltas and terminal text in one answer", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(envelope({ type: "content_append", text: "Hello " }), state)).toEqual([
      { kind: "answer_delta", text: "Hello " },
    ]);
    expect(projectStreamEvent(envelope({ type: "final", text: "Hello world" }), state)).toEqual([
      { kind: "answer_delta", text: "world" },
      { kind: "terminal", error: false },
    ]);
  });

  it("treats workshop handoff as a foreground boundary", () => {
    const projected = projectStreamEvent(
      envelope({
        type: "worker_ack",
        ack_kind: "workshop",
        text: "I’m taking this into the workshop.",
        work_id: "work-one",
      }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({
      kind: "handoff",
      text: "Medousa is in the workshop",
      workId: "work-one",
    });
    expect(projected.some((item) => item.kind === "terminal")).toBe(false);
  });

  it("renders approval requests without losing the active stream", () => {
    const projected = projectStreamEvent(
      envelope({
        type: "budget_approval_required",
        request_id: "budget-one",
        requested_rounds: 2,
        rounds_executed: 8,
        max_tool_rounds: 8,
        reason: "Need another lookup",
      }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({
      kind: "budget_request",
      requestId: "budget-one",
      rounds: 2,
    });
  });
});
