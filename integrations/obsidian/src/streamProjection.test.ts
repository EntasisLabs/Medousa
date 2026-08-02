import { describe, expect, it } from "vitest";
import type { InteractiveTurnStreamEvent } from "@medousa/client";
import { createProjectionState, projectStreamEvent } from "./streamProjection";

function event(overrides: Partial<InteractiveTurnStreamEvent>): InteractiveTurnStreamEvent {
  return {
    turn_id: "turn-1",
    seq: 1,
    event_type: "status",
    phase: "working",
    message: "",
    terminal: false,
    emitted_at_utc: "now",
    ...overrides,
  };
}

describe("Obsidian stream projection", () => {
  it("does not duplicate a final answer after streamed deltas", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(event({ event_type: "content", content_delta: "Hello" }), state)).toEqual([
      { kind: "answer_delta", text: "Hello" },
    ]);
    expect(projectStreamEvent(event({ event_type: "done", terminal: true, final_text: "Hello" }), state)).toContainEqual({
      kind: "terminal",
      error: false,
    });
    expect(projectStreamEvent(event({ event_type: "done", terminal: true, final_text: "Hello" }), state)).not.toContainEqual({
      kind: "answer_delta",
      text: "Hello",
    });
  });

  it("projects approval requests without exposing engine telemetry", () => {
    const projected = projectStreamEvent(
      event({
        budget_request_id: "budget-1",
        requested_rounds: 2,
        operator_message: "interactive turn accepted; agent runtime started",
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
      event({
        event_type: "workshop_ack",
        phase: "workshop_ack",
        operator_message: "Medousa is in the workshop",
        final_text: "I’m taking this into the workshop.",
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
