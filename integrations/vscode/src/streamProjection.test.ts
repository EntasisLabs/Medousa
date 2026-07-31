import { describe, expect, it } from "vitest";
import type { InteractiveTurnStreamEvent } from "@medousa/client";
import { createProjectionState, projectStreamEvent } from "./streamProjection.js";

function event(overrides: Partial<InteractiveTurnStreamEvent>): InteractiveTurnStreamEvent {
  return {
    turn_id: "t1",
    seq: 1,
    event_type: "status",
    phase: "working",
    message: "",
    terminal: false,
    emitted_at_utc: "now",
    ...overrides,
  };
}

describe("VS Code stream projection", () => {
  it("hides daemon lifecycle and context telemetry", () => {
    const state = createProjectionState();
    const projected = projectStreamEvent(
      event({ message: "interactive turn accepted; agent runtime started" }),
      state,
    );
    expect(projected).toEqual([]);
  });

  it("turns tool lifecycle into structured events", () => {
    const state = createProjectionState();
    const started = projectStreamEvent(
      event({
        event_type: "tool_started",
        tool_name: "cognition_vault_search",
        tool_run_id: "run-1",
        tool_status: "running",
      }),
      state,
    );
    const finished = projectStreamEvent(
      event({
        event_type: "tool_finished",
        tool_name: "cognition_vault_search",
        tool_run_id: "run-1",
        tool_status: "succeeded",
      }),
      state,
    );
    expect(started[0]).toMatchObject({ kind: "tool_started", name: "Vault Search" });
    expect(finished[0]).toMatchObject({ kind: "tool_finished", status: "succeeded" });
  });

  it("does not duplicate final text after streamed deltas", () => {
    const state = createProjectionState();
    expect(projectStreamEvent(event({ event_type: "content", content_delta: "Hello" }), state)).toEqual([
      { kind: "answer_delta", text: "Hello" },
    ]);
    expect(
      projectStreamEvent(event({ event_type: "done", terminal: true, final_text: "Hello" }), state),
    ).not.toContainEqual({ kind: "answer_delta", text: "Hello" });
  });

  it("uses final text when no deltas arrived", () => {
    const projected = projectStreamEvent(
      event({ event_type: "done", terminal: true, final_text: "A complete answer" }),
      createProjectionState(),
    );
    expect(projected).toContainEqual({ kind: "answer_delta", text: "A complete answer" });
  });

  it("replaces interim prose with final synthesis after tool use", () => {
    const state = createProjectionState();
    projectStreamEvent(event({ event_type: "content", content_delta: "Let me check." }), state);
    projectStreamEvent(event({
      event_type: "tool_started",
      tool_name: "cognition_vault_search",
      tool_run_id: "run-1",
      tool_status: "running",
    }), state);
    const projected = projectStreamEvent(
      event({ event_type: "done", terminal: true, final_text: "Here is the result." }),
      state,
    );
    expect(projected).toContainEqual({ kind: "answer_replace", text: "Here is the result." });
  });
});
