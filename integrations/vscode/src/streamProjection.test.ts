import { describe, expect, it } from "vitest";
import type { TurnStreamEnvelopeV2, TurnStreamEventV2 } from "@medousa/client";
import { createProjectionState, projectStreamEvent } from "./streamProjection.js";

function envelope(event: TurnStreamEventV2): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: "t1",
    seq: 1,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    event,
  };
}

describe("VS Code stream projection", () => {
  it("hides daemon lifecycle and context telemetry", () => {
    const projected = projectStreamEvent(
      envelope({
        type: "status",
        phase: "working",
        operator_message: "interactive turn accepted; agent runtime started",
      }),
      createProjectionState(),
    );
    expect(projected).toEqual([]);
  });

  it("turns tool lifecycle into structured events", () => {
    const state = createProjectionState();
    const started = projectStreamEvent(
      envelope({
        type: "tool_started",
        tool_name: "cognition_vault_search",
        tool_run_id: "run-1",
        tool_round: 1,
        input_summary: "query vault",
      }),
      state,
    );
    const finished = projectStreamEvent(
      envelope({
        type: "tool_finished",
        tool_name: "cognition_vault_search",
        tool_run_id: "run-1",
        tool_round: 1,
        input_summary: "query vault",
        status: "succeeded",
      }),
      state,
    );
    expect(started[0]).toMatchObject({ kind: "tool_started", name: "Vault Search" });
    expect(finished[0]).toMatchObject({ kind: "tool_finished", status: "succeeded" });
  });

  it("does not duplicate final text after streamed deltas", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(envelope({ type: "content_append", text: "Hello" }), state)).toEqual([
      { kind: "answer_delta", text: "Hello" },
    ]);
    expect(projectStreamEvent(envelope({ type: "final", text: "Hello" }), state)).not.toContainEqual({
      kind: "answer_delta",
      text: "Hello",
    });
  });

  it("uses final text when no deltas arrived", () => {
    const projected = projectStreamEvent(
      envelope({ type: "final", text: "A complete answer" }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({ kind: "answer_delta", text: "A complete answer" });
  });

  it("replaces interim prose with final synthesis after tool use", () => {
    const state = createProjectionState();
    projectStreamEvent(envelope({ type: "content_append", text: "Let me check." }), state);
    projectStreamEvent(
      envelope({
        type: "tool_started",
        tool_name: "cognition_vault_search",
        tool_run_id: "run-1",
        tool_round: 1,
        input_summary: "query vault",
      }),
      state,
    );
    const projected = projectStreamEvent(
      envelope({ type: "final", text: "Here is the result." }),
      state,
    );
    expect(projected).toContainEqual({ kind: "answer_replace", text: "Here is the result." });
  });

  it("releases the host turn when work enters the workshop", () => {
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
    expect(projected).not.toContainEqual({ kind: "terminal", error: false });
  });
});
