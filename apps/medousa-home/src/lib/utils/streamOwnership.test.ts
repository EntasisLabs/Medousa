import { describe, expect, it } from "vitest";
import type { TurnTicketState } from "$lib/types/chat";
import {
  shouldAcceptStreamEvent,
  shouldReattachTurnRecord,
  type StreamOwner,
} from "./streamOwnership";

describe("shouldReattachTurnRecord", () => {
  const baseCtx = {
    principalSessionId: "session-a",
    isRelevantSession: () => true,
    isDetachedWorkerTurn: () => false,
    hasAssistantMessage: true,
    assistantStreaming: false,
  };

  it("reattaches when daemon ticket is still live but UI bubble is not streaming", () => {
    expect(
      shouldReattachTurnRecord(
        {
          turn_id: "turn-1",
          session_id: "session-a",
          mode: "interactive",
          phase: "streaming",
          stream_url: "/stream",
          prompt_preview: "",
          workspace_card_id: null,
          composer_handoff: false,
          started_at: "",
          updated_at: "",
        },
        { ...baseCtx, localTurn: undefined },
      ),
    ).toBe(true);
  });

  it("refuses reattach when daemon ticket is terminal", () => {
    expect(
      shouldReattachTurnRecord(
        {
          turn_id: "turn-1",
          session_id: "session-a",
          mode: "interactive",
          phase: "done",
          stream_url: "/stream",
          prompt_preview: "",
          workspace_card_id: null,
          composer_handoff: false,
          started_at: "",
          updated_at: "",
        },
        { ...baseCtx, localTurn: undefined },
      ),
    ).toBe(false);
  });
});

describe("shouldAcceptStreamEvent", () => {
  const owners = new Map<string, StreamOwner>();
  const turns = new Map<string, TurnTicketState>();

  it("accepts late frames for recently settled turns", () => {
    expect(
      shouldAcceptStreamEvent("turn-1", owners, turns, {
        recentlySettledTurnIds: new Set(["turn-1"]),
      }),
    ).toBe(true);
  });

  it("accepts observer-window frames for transcript turn ids", () => {
    expect(
      shouldAcceptStreamEvent("turn-2", owners, turns, {
        transcriptTurnIds: new Set(["turn-2"]),
      }),
    ).toBe(true);
  });

  it("rejects unknown turn ids without context", () => {
    expect(shouldAcceptStreamEvent("turn-3", owners, turns)).toBe(false);
  });
});
