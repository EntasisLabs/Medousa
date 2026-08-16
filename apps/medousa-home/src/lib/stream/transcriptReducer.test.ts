import { describe, expect, it } from "vitest";

import { reduceTranscriptEnvelope } from "$lib/stream/transcriptReducer";
import type { ChatMessage } from "$lib/types/chat";
import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";

function envelope(
  seq: number,
  event: TurnStreamEventV2,
  turnId = "turn-1",
): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: turnId,
    seq,
    emitted_at_utc: "2026-08-16T00:00:00Z",
    event,
  };
}

function assistant(content = ""): ChatMessage {
  return {
    id: "asst-1",
    role: "assistant",
    content,
    streaming: true,
    turnId: "turn-1",
  };
}

const ctx = {
  messageIdForTurn: () => "asst-1",
  messageIdForToolStream: () => "asst-1",
  showEngineDetails: false,
};

describe("reduceTranscriptEnvelope", () => {
  it("appends v2 content without Svelte", () => {
    const result = reduceTranscriptEnvelope(
      [assistant()],
      envelope(1, { type: "content_append", text: "Hello" }),
      ctx,
    );
    expect(result.handled).toBe(true);
    expect(result.messages[0]?.content).toBe("Hello");
    expect(result.legacy.event_type).toBe("content_delta");
  });

  it("appends reasoning on the same assistant bubble", () => {
    const first = reduceTranscriptEnvelope(
      [assistant("Hi")],
      envelope(1, { type: "content_append", text: " there" }),
      ctx,
    );
    const second = reduceTranscriptEnvelope(
      first.messages,
      envelope(2, { type: "reasoning_append", text: "think" }),
      ctx,
    );
    expect(second.handled).toBe(true);
    expect(second.messages[0]?.content).toBe("Hi there");
    expect(second.messages[0]?.reasoning).toBe("think");
  });

  it("records tool_started on the turn message", () => {
    const result = reduceTranscriptEnvelope(
      [assistant()],
      envelope(3, {
        type: "tool_started",
        tool_run_id: "run-1",
        tool_name: "search",
        input_summary: "query",
        tool_round: 1,
      }),
      ctx,
    );
    expect(result.handled).toBe(true);
    expect(result.messages[0]?.toolRuns).toEqual([
      expect.objectContaining({
        runId: "run-1",
        toolName: "search",
        status: "running",
      }),
    ]);
  });

  it("delegates terminal envelopes to the store", () => {
    const result = reduceTranscriptEnvelope(
      [assistant("draft")],
      envelope(9, { type: "final", text: "done" }),
      ctx,
    );
    expect(result.handled).toBe(false);
    expect(result.messages[0]?.content).toBe("draft");
  });
});
