import { describe, expect, it } from "vitest";
import type { TurnStreamEnvelopeV2, TurnStreamEventV2 } from "@medousa/client";
import { createProjectionState, projectStreamEvent } from "./streamProjection";

function envelope(event: TurnStreamEventV2): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: "turn-1",
    seq: 1,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    event,
  };
}

describe("Obsidian stream projection", () => {
  it("does not duplicate a final answer after streamed deltas", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(envelope({ type: "content_append", text: "Hello" }), state)).toEqual([
      { kind: "answer_delta", text: "Hello" },
    ]);
    const terminal = projectStreamEvent(envelope({ type: "final", text: "Hello" }), state);
    expect(terminal).toContainEqual({ kind: "terminal", error: false });
    expect(terminal).not.toContainEqual({ kind: "answer_delta", text: "Hello" });
  });

  it("projects approval requests without exposing engine telemetry", () => {
    const projected = projectStreamEvent(
      envelope({
        type: "budget_approval_required",
        request_id: "budget-1",
        requested_rounds: 2,
        rounds_executed: 8,
        max_tool_rounds: 8,
        reason: "Need another lookup",
        progress_summary: "interactive turn accepted; agent runtime started",
      }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({ kind: "budget_request", requestId: "budget-1", rounds: 2 });
    expect(projected).not.toContainEqual({
      kind: "status",
      text: "interactive turn accepted; agent runtime started",
    });
  });

  it("marks workshop handoff as a composer boundary", () => {
    const projected = projectStreamEvent(
      envelope({
        type: "worker_ack",
        ack_kind: "workshop",
        text: "I’m taking this into the workshop.",
        work_id: "work-1",
      }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({
      kind: "handoff",
      text: "Medousa is in the workshop",
      workId: "work-1",
    });
  });
});
