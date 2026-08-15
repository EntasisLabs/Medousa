import { describe, expect, it } from "vitest";

import {
  turnStreamPayloadToLegacy,
  turnStreamPayloadToV2,
  turnStreamV2ToLegacy,
} from "$lib/stream/v2ToLegacy";
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

const legacyTypes = [
  "content_delta",
  "reasoning_delta",
  "status",
  "turn_progress",
  "assistant_pack_hold",
  "model_receipt",
  "final",
  "needs_input",
  "turn_checkpoint",
  "worker_ack",
  "worker_synthesis",
  "final_pending",
  "error",
  "scratch_reset",
  "tool_started",
  "tool_finished",
  "artifact_presented",
  "artifact_updated",
  "ui_scene",
  "budget_approval",
  "browser_challenge",
  "browser_navigated",
  "context_usage",
  "permission_request",
];

function envelope(event: TurnStreamEventV2, seq: number): TurnStreamEnvelopeV2 {
  return {
    schema_version: 2,
    turn_id: "turn-1",
    seq,
    emitted_at_utc: "2026-08-15T00:00:00Z",
    event,
  };
}

describe("turnStreamV2ToLegacy", () => {
  it("projects every generated union variant through the production seam", () => {
    const projected = variants.map((event, index) =>
      turnStreamV2ToLegacy(envelope(event, index + 1)),
    );

    expect(projected.map((event) => event.event_type)).toEqual(legacyTypes);
    expect(projected.every((event) => event.turn_id === "turn-1")).toBe(true);
    expect(projected.map((event) => event.seq)).toEqual(
      variants.map((_, index) => index + 1),
    );
  });

  it("preserves append, terminal, and tool payload semantics", () => {
    expect(turnStreamV2ToLegacy(envelope(variants[0], 1))).toMatchObject({
      content_delta: "hello",
      terminal: false,
    });
    const terminal = turnStreamV2ToLegacy(envelope(variants[6], 2));
    expect(terminal).toMatchObject({
      final_text: "done",
      terminal: true,
    });
    expect(terminal).not.toHaveProperty("operator_message");
    expect(turnStreamV2ToLegacy(envelope(variants[14], 3))).toMatchObject({
      tool_run_id: "run-1",
      tool_name: "search",
      tool_status: "running",
      tool_round: 1,
    });
  });

  it("passes a legacy payload through during the compatibility window", () => {
    const legacy = turnStreamV2ToLegacy(envelope(variants[0], 1));
    expect(turnStreamPayloadToLegacy(legacy)).toBe(legacy);
  });

  it("keeps v2 payloads allocation-free and projects legacy payloads only on fallback", () => {
    const current = envelope(variants[0], 1);
    expect(turnStreamPayloadToV2(current)).toBe(current);

    const legacy = turnStreamV2ToLegacy(envelope(variants[14], 3));
    expect(turnStreamPayloadToV2(legacy)).toMatchObject(envelope(variants[14], 3));
  });
});
