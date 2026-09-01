import { describe, expect, it } from "vitest";

import {
  applyV3EnvelopeToMessage,
  foldV3Envelopes,
  v3EventPromotesChatMessage,
} from "$lib/stream/v3TranscriptReducer";
import type { ChatMessage } from "$lib/types/chat";
import type {
  TurnStreamEnvelopeV3,
  TurnStreamEventV3,
} from "$lib/types/generated/daemon_api";

function envelope(
  seq: number,
  event: TurnStreamEventV3,
  turnId = "turn-1",
): TurnStreamEnvelopeV3 {
  return {
    schema_version: 3,
    turn_id: turnId,
    seq,
    emitted_at_utc: "2026-08-25T00:00:00Z",
    event,
  };
}

function assistant(): ChatMessage {
  return {
    id: "asst-1",
    role: "assistant",
    content: "",
    streaming: true,
    turnId: "turn-1",
  };
}

function textEvents(
  firstSeq: number,
  segmentId: string,
  modelRound: number,
  text: string,
): TurnStreamEnvelopeV3[] {
  return [
    envelope(firstSeq, {
      type: "assistant_text_started",
      segment_id: segmentId,
      model_round: modelRound,
    }),
    envelope(firstSeq + 1, { type: "content_append", segment_id: segmentId, text }),
    envelope(firstSeq + 2, { type: "assistant_text_committed", segment_id: segmentId }),
  ];
}

function toolStarted(
  seq: number,
  runId: string,
  toolName: string,
  toolRound: number,
): TurnStreamEnvelopeV3 {
  return envelope(seq, {
    type: "tool_started",
    tool_run_id: runId,
    tool_name: toolName,
    tool_round: toolRound,
    input_summary: `${toolName} input`,
  });
}

function toolFinished(
  seq: number,
  runId: string,
  toolName: string,
  toolRound: number,
  status = "succeeded",
): TurnStreamEnvelopeV3 {
  return envelope(seq, {
    type: "tool_finished",
    tool_run_id: runId,
    tool_name: toolName,
    tool_round: toolRound,
    input_summary: `${toolName} input`,
    output_summary: `${toolName} output`,
    status,
  });
}

describe("Turn Stream V3 chronological presentation fold", () => {
  it("promotes tool-first and text-first reconnect facts into chat", () => {
    expect(v3EventPromotesChatMessage(toolStarted(1, "run-1", "search", 1).event)).toBe(true);
    expect(
      v3EventPromotesChatMessage({
        type: "assistant_text_started",
        segment_id: "segment-a",
        model_round: 1,
      }),
    ).toBe(true);
    expect(v3EventPromotesChatMessage({ type: "model_receipt", provider: "p", model: "m" }))
      .toBe(false);
    expect(
      v3EventPromotesChatMessage({
        type: "progress",
        message: "Checking the durable transcript.",
      }),
    ).toBe(true);
  });

  it("keeps turn-control progress as a chronological timeline message", () => {
    const result = foldV3Envelopes(assistant(), [
      toolStarted(1, "run-1", "cognition_turn", 1),
      toolFinished(2, "run-1", "cognition_turn", 1),
      envelope(3, {
        type: "progress",
        message: "Checking the durable transcript.",
        tool_names: ["cognition_turn"],
      }),
      ...textEvents(4, "segment-a", 2, "The transcript is intact."),
    ]);

    expect(result.segments?.map((segment) => segment.kind)).toEqual([
      "tool_group",
      "progress",
      "text",
    ]);
    expect(result.segments?.[1]).toEqual({
      kind: "progress",
      progressId: "progress:turn-1:3",
      markdown: "Checking the durable transcript.",
    });
    expect(result.statusLine).toBeNull();
  });

  it("keeps response → parallel tools → response → tools → response in occurrence order", () => {
    const events = [
      ...textEvents(1, "segment-a", 1, "Let me check."),
      toolStarted(4, "run-1", "vault.read", 1),
      toolStarted(5, "run-2", "web.search", 1),
      toolStarted(6, "run-3", "files.inspect", 1),
      // Completion timing must not reorder the declared runs.
      toolFinished(7, "run-3", "files.inspect", 1),
      toolFinished(8, "run-1", "vault.read", 1),
      toolFinished(9, "run-2", "web.search", 1),
      ...textEvents(10, "segment-b", 2, "I found a lead."),
      toolStarted(13, "run-4", "vault.read", 2),
      toolStarted(14, "run-5", "code.search", 2),
      toolFinished(15, "run-5", "code.search", 2),
      toolFinished(16, "run-4", "vault.read", 2),
      ...textEvents(17, "segment-c", 3, "Here is the answer."),
    ];

    const beforeTerminal = foldV3Envelopes(assistant(), events);
    const result = applyV3EnvelopeToMessage(
      beforeTerminal,
      envelope(20, {
        type: "turn_completed",
        outcome: "completed",
        // Aggregate text is compatibility/search data, not the rendering source.
        aggregate_text: "authoritative-looking replacement",
      }),
    );

    expect(result.segments?.map((segment) => segment.kind)).toEqual([
      "text",
      "tool_group",
      "text",
      "tool_group",
      "text",
    ]);
    expect(result.segments).toEqual(beforeTerminal.segments);
    expect(result.segments?.[1]).toEqual(
      expect.objectContaining({
        kind: "tool_group",
        runs: [
          expect.objectContaining({ runId: "run-1", status: "succeeded" }),
          expect.objectContaining({ runId: "run-2", status: "succeeded" }),
          expect.objectContaining({ runId: "run-3", status: "succeeded" }),
        ],
      }),
    );
    expect(result.content).toBe("Let me check.\n\nI found a lead.\n\nHere is the answer.");
    expect(result.toolRuns?.map((run) => run.runId)).toEqual([
      "run-1",
      "run-2",
      "run-3",
      "run-4",
      "run-5",
    ]);
    expect(result.streaming).toBe(false);
    expect(result.answerState).toBe("completed");
  });

  it("keeps a failed receipt in place before recovery prose and later tools", () => {
    const result = foldV3Envelopes(assistant(), [
      toolStarted(1, "run-1", "web.search", 1),
      toolFinished(2, "run-1", "web.search", 1, "failed"),
      ...textEvents(3, "segment-a", 2, "That failed; I will try locally."),
      toolStarted(6, "run-2", "files.inspect", 2),
      toolFinished(7, "run-2", "files.inspect", 2),
    ]);

    expect(result.segments?.map((segment) => segment.kind)).toEqual([
      "tool_group",
      "text",
      "tool_group",
    ]);
    expect(result.toolRuns?.map((run) => run.status)).toEqual(["failed", "succeeded"]);
  });

  it("coalesces consecutive tool rounds until visible assistant prose splits them", () => {
    const result = foldV3Envelopes(assistant(), [
      toolStarted(1, "run-1", "mcp.list_tools", 1),
      toolFinished(2, "run-1", "mcp.list_tools", 1),
      toolStarted(3, "run-2", "mcp.inspect", 2),
      toolFinished(4, "run-2", "mcp.inspect", 2),
      toolStarted(5, "run-3", "mcp.read", 3),
      toolFinished(6, "run-3", "mcp.read", 3),
      ...textEvents(7, "segment-a", 4, "I found what I needed."),
      toolStarted(10, "run-4", "mcp.verify", 4),
      toolFinished(11, "run-4", "mcp.verify", 4),
    ]);

    expect(result.segments?.map((segment) => segment.kind)).toEqual([
      "tool_group",
      "text",
      "tool_group",
    ]);
    expect(result.segments?.[0]).toEqual(
      expect.objectContaining({
        kind: "tool_group",
        runs: [
          expect.objectContaining({ runId: "run-1", round: 1 }),
          expect.objectContaining({ runId: "run-2", round: 2 }),
          expect.objectContaining({ runId: "run-3", round: 3 }),
        ],
      }),
    );
    expect(result.segments?.[2]).toEqual(
      expect.objectContaining({
        kind: "tool_group",
        runs: [expect.objectContaining({ runId: "run-4", round: 4 })],
      }),
    );
  });

  it("preserves partial prose when the terminal outcome is a failure", () => {
    const partial = foldV3Envelopes(
      assistant(),
      textEvents(1, "segment-a", 1, "I confirmed the first half."),
    );
    const result = applyV3EnvelopeToMessage(
      partial,
      envelope(4, {
        type: "turn_completed",
        outcome: "failed",
        aggregate_text: "I confirmed the first half.",
        operator_message: "The remaining check failed.",
        debug_message: "provider timeout",
      }),
    );

    expect(result.content).toBe("I confirmed the first half.");
    expect(result.segments).toEqual(partial.segments);
    expect(result.failed).toBe(true);
    expect(result.errorLine).toBe("The remaining check failed.");
    expect(result.errorDetail).toBe("provider timeout");
  });

  it("does not turn terminal aggregate text into a synthetic segment", () => {
    const message = { ...assistant(), content: "locally retained text" };
    const result = applyV3EnvelopeToMessage(
      message,
      envelope(4, {
        type: "turn_completed",
        outcome: "completed",
        aggregate_text: "flattened compatibility text",
      }),
    );

    expect(result.content).toBe("locally retained text");
    expect(result.segments).toBeUndefined();
    expect(result.streaming).toBe(false);
  });

  it("can use a reconnect suffix without requiring the segment-start prefix", () => {
    const result = foldV3Envelopes(assistant(), [
      envelope(9, { type: "content_append", segment_id: "segment-existing", text: "suffix" }),
      envelope(10, { type: "assistant_text_committed", segment_id: "segment-existing" }),
      toolFinished(11, "run-existing", "vault.read", 3),
    ]);

    expect(result.segments).toEqual([
      {
        kind: "text",
        segmentId: "segment-existing",
        modelRound: null,
        markdown: "suffix",
        committed: true,
      },
      expect.objectContaining({
        kind: "tool_group",
        toolRound: 3,
        runs: [expect.objectContaining({ runId: "run-existing", status: "succeeded" })],
      }),
    ]);
  });

  it("keeps artifacts at their observed position and updates revisions in place", () => {
    const first = foldV3Envelopes(assistant(), [
      ...textEvents(1, "segment-a", 1, "Opening the chart."),
      envelope(4, {
        type: "artifact_presented",
        artifact: {
          artifact_id: "artifact-v1",
          mime: "text/html",
          label: "Chart",
          presentation: "panel",
        },
      }),
      ...textEvents(5, "segment-b", 2, "I updated it."),
    ]);
    const result = applyV3EnvelopeToMessage(
      first,
      envelope(8, {
        type: "artifact_updated",
        previous_artifact_id: "artifact-v1",
        root_artifact_id: "artifact-root",
        artifact: {
          artifact_id: "artifact-v2",
          mime: "text/html",
          label: "Chart v2",
          presentation: "panel",
        },
      }),
    );

    expect(result.segments?.map((segment) => segment.kind)).toEqual([
      "text",
      "artifact",
      "text",
    ]);
    expect(result.uiArtifacts?.[0]).toEqual(
      expect.objectContaining({ artifactId: "artifact-v2", rootArtifactId: "artifact-root" }),
    );
  });
});
