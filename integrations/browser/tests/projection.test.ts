import { describe, expect, it } from "vitest";
import { createProjectionState, projectStreamEvent } from "../src/projection.js";

function event(overrides: Record<string, unknown> = {}) {
  return {
    emitted_at_utc: "2026-08-02T00:00:00Z",
    event_type: "content",
    message: "",
    phase: "content",
    terminal: false,
    turn_id: "turn-one",
    ...overrides,
  } as never;
}

describe("browser stream projection", () => {
  it("keeps deltas and terminal text in one answer", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(event({ content_delta: "Hello " }), state)).toEqual([
      { kind: "answer_delta", text: "Hello " },
    ]);
    expect(projectStreamEvent(event({ terminal: true, final_text: "Hello world" }), state)).toEqual([
      { kind: "answer_replace", text: "Hello world" },
      { kind: "terminal", text: undefined, error: false },
    ]);
  });

  it("treats workshop handoff as a foreground boundary", () => {
    const projected = projectStreamEvent(event({
      event_type: "workshop_ack",
      phase: "workshop_ack",
      operator_message: "Medousa is in the workshop",
      work_id: "work-one",
    }), createProjectionState());
    expect(projected).toContainEqual({
      kind: "handoff",
      text: "Medousa is in the workshop",
      workId: "work-one",
    });
    expect(projected.some((item) => item.kind === "terminal")).toBe(false);
  });

  it("renders approval requests without losing the active stream", () => {
    const projected = projectStreamEvent(event({
      event_type: "budget_request",
      budget_request_id: "budget-one",
      requested_rounds: 2,
    }), createProjectionState());
    expect(projected).toContainEqual({ kind: "budget_request", requestId: "budget-one", rounds: 2 });
  });
});
