import { describe, expect, it } from "vitest";

import { emptyTurnStreamV2State, reduceTurnStreamV2 } from "$lib/stream/v2Reducer";
import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";

const variants = [
  { type: "content_append", text: "hello" },
  { type: "reasoning_append", text: "thought" },
  { type: "status", phase: "working" },
  { type: "progress", message: "progress" },
  { type: "pack_hold", text: "held" },
  { type: "model_receipt", provider: "openai", model: "model" },
  { type: "final", text: "done" },
  { type: "needs_input", text: "question" },
  { type: "checkpoint", text: "checkpoint" },
  { type: "worker_ack", ack_kind: "worker", text: "started" },
  { type: "worker_synthesis", text: "result" },
  { type: "final_pending", text: "wrapping" },
  { type: "error", operator_message: "failed" },
  { type: "scratch_reset" },
  {
    type: "tool_started",
    tool_run_id: "run-1",
    tool_name: "search",
    input_summary: "query",
    tool_round: 1,
  },
  {
    type: "tool_finished",
    tool_run_id: "run-1",
    tool_name: "search",
    status: "ok",
    input_summary: "query",
    tool_round: 1,
  },
  {
    type: "artifact_presented",
    artifact: { artifact_id: "a1", mime: "text/html", label: "A", presentation: "inline" },
  },
  {
    type: "artifact_updated",
    previous_artifact_id: "a1",
    artifact: { artifact_id: "a2", mime: "text/html", label: "A", presentation: "inline" },
  },
  { type: "ui_scene", scene: { ops: [] } },
  {
    type: "budget_approval_required",
    request_id: "b1",
    rounds_executed: 10,
    max_tool_rounds: 10,
    requested_rounds: 5,
    reason: "continue",
  },
  {
    type: "browser_challenge",
    session_id: "browser-1",
    challenge_url: "https://example.invalid",
    reason: "captcha",
  },
  { type: "browser_navigated", url: "https://example.invalid" },
  {
    type: "context_usage",
    report: {
      layers: [],
      total_tokens_estimate: 10,
      total_chars: 40,
      tool_count: 2,
      estimator: "chars/4",
    },
  },
  { type: "permission_request", request_id: "p1", message: "Allow?" },
] satisfies TurnStreamEventV2[];

function envelope(event: TurnStreamEventV2, seq: number): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: "turn-1",
    seq,
    emitted_at_utc: "2026-08-14T00:00:00Z",
    event,
  };
}

describe("turn stream v2 reducer", () => {
  it("handles every generated union variant", () => {
    const state = variants.reduce(
      (current, event, index) => {
        const encoded = JSON.stringify(envelope(event, index + 1));
        const decoded = JSON.parse(encoded) as TurnStreamEnvelopeV2;
        expect(JSON.stringify(decoded)).toBe(encoded);
        return reduceTurnStreamV2(current, decoded);
      },
      emptyTurnStreamV2State(),
    );
    expect(state.seq).toBe(variants.length);
  });

  it("concatenates batches in sequence and ignores replay duplicates", () => {
    const first = reduceTurnStreamV2(
      emptyTurnStreamV2State(),
      envelope({ type: "content_append", text: "hello " }, 1),
    );
    const second = reduceTurnStreamV2(
      first,
      envelope({ type: "content_append", text: "world" }, 2),
    );
    expect(second.content).toBe("hello world");
    expect(reduceTurnStreamV2(second, envelope({ type: "content_append", text: "bad" }, 2))).toBe(
      second,
    );
  });
});
