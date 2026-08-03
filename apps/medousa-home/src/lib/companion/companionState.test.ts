import { describe, expect, it } from "vitest";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import {
  applyCompanionStreamEvent,
  companionSpriteState,
  initialCompanionActivity,
} from "./companionState";

function event(
  partial: Partial<InteractiveTurnStreamEvent>,
): InteractiveTurnStreamEvent {
  return {
    turn_id: "turn-1",
    event_type: "status",
    phase: "streaming",
    message: "Working",
    terminal: false,
    emitted_at_utc: "2026-08-02T00:00:00Z",
    ...partial,
  };
}

describe("applyCompanionStreamEvent", () => {
  it("tracks active work and settles on success", () => {
    const working = applyCompanionStreamEvent(
      initialCompanionActivity(),
      event({ event_type: "tool_started" }),
    );
    expect(working.activeTurnIds.has("turn-1")).toBe(true);

    const done = applyCompanionStreamEvent(
      working,
      event({ terminal: true, final_text: "Finished the task" }),
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
        event_type: "budget_approval",
        phase: "budget_blocked",
        operator_message: "Approve two more tool rounds",
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
