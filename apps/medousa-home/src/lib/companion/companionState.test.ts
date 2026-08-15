import { describe, expect, it } from "vitest";
import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";
import {
  applyCompanionStreamEvent,
  companionSpriteState,
  initialCompanionActivity,
} from "./companionState";

function event(
  payload: TurnStreamEventV2,
): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: "turn-1",
    seq: 1,
    emitted_at_utc: "2026-08-02T00:00:00Z",
    event: payload,
  };
}

describe("applyCompanionStreamEvent", () => {
  it("tracks active work and settles on success", () => {
    const working = applyCompanionStreamEvent(
      initialCompanionActivity(),
      event({
        type: "tool_started",
        tool_run_id: "run-1",
        tool_name: "search",
        input_summary: "query",
        tool_round: 1,
      }),
    );
    expect(working.activeTurnIds.has("turn-1")).toBe(true);

    const done = applyCompanionStreamEvent(
      working,
      event({ type: "final", text: "Finished the task" }),
    );
    expect(done.activeTurnIds.size).toBe(0);
    expect(done.feedback).toEqual({
      tone: "success",
      message: "Finished the task",
    });
  });

  it("turns a budget pause into attention feedback", () => {
    const result = applyCompanionStreamEvent(
      initialCompanionActivity(),
      event({
        type: "budget_approval_required",
        request_id: "approval-1",
        rounds_executed: 4,
        max_tool_rounds: 4,
        requested_rounds: 2,
        reason: "more work remains",
        progress_summary: "Approve two more tool rounds",
      }),
    );
    expect(result.approvalChanged).toBe(true);
    expect(result.feedback?.tone).toBe("attention");
  });
});

describe("companionSpriteState", () => {
  it("prioritizes connection, approvals, sending, and work", () => {
    const base = {
      connected: true,
      expanded: false,
      sending: false,
      activeTurnCount: 0,
      pendingApproval: false,
      feedbackTone: null,
    } as const;
    expect(companionSpriteState(base)).toBe("float");
    expect(companionSpriteState({ ...base, activeTurnCount: 1 })).toBe("loading");
    expect(companionSpriteState({ ...base, sending: true })).toBe("launch");
    expect(companionSpriteState({ ...base, pendingApproval: true })).toBe("attention");
    expect(companionSpriteState({ ...base, connected: false })).toBe("error");
  });
});
