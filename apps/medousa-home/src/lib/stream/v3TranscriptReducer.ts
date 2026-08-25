/**
 * Medousa's presentation fold for raw Turn Stream V3 facts.
 *
 * The protocol remains a freely consumable fact stream. This reducer owns only
 * Medousa's chronological chat projection and deliberately treats `seq` as a
 * reconnect/deduplication cursor, never as presentation identity.
 */

import { mapStreamUiArtifact } from "$lib/types/artifact";
import type { ChatMessage, ChatSegment, ToolRunState, UiArtifact } from "$lib/types/chat";
import type {
  TurnStreamEnvelopeV3,
  TurnStreamEventV3,
} from "$lib/types/generated/daemon_api";

function replaceSegment(
  segments: ChatSegment[],
  index: number,
  segment: ChatSegment,
): ChatSegment[] {
  const next = [...segments];
  next[index] = segment;
  return next;
}

function textIndex(segments: ChatSegment[], segmentId: string): number {
  return segments.findIndex(
    (segment) => segment.kind === "text" && segment.segmentId === segmentId,
  );
}

function applyTextStarted(
  segments: ChatSegment[],
  event: Extract<TurnStreamEventV3, { type: "assistant_text_started" }>,
): ChatSegment[] {
  const index = textIndex(segments, event.segment_id);
  if (index < 0) {
    return [
      ...segments,
      {
        kind: "text",
        segmentId: event.segment_id,
        modelRound: event.model_round,
        markdown: "",
        committed: false,
      },
    ];
  }
  const current = segments[index];
  if (current.kind !== "text") return segments;
  return replaceSegment(segments, index, {
    ...current,
    modelRound: event.model_round,
  });
}

function applyContentAppend(
  segments: ChatSegment[],
  event: Extract<TurnStreamEventV3, { type: "content_append" }>,
): ChatSegment[] {
  const index = textIndex(segments, event.segment_id);
  if (index < 0) {
    // A reconnect suffix can begin after AssistantTextStarted. The append is
    // still independently useful without inventing a model round.
    return [
      ...segments,
      {
        kind: "text",
        segmentId: event.segment_id,
        modelRound: null,
        markdown: event.text,
        committed: false,
      },
    ];
  }
  const current = segments[index];
  if (current.kind !== "text") return segments;
  return replaceSegment(segments, index, {
    ...current,
    markdown: current.markdown + event.text,
  });
}

function applyTextCommitted(
  segments: ChatSegment[],
  event: Extract<TurnStreamEventV3, { type: "assistant_text_committed" }>,
): ChatSegment[] {
  const index = textIndex(segments, event.segment_id);
  if (index < 0) {
    return [
      ...segments,
      {
        kind: "text",
        segmentId: event.segment_id,
        modelRound: null,
        markdown: "",
        committed: true,
      },
    ];
  }
  const current = segments[index];
  if (current.kind !== "text") return segments;
  return replaceSegment(segments, index, { ...current, committed: true });
}

function runFromToolEvent(
  event: Extract<TurnStreamEventV3, { type: "tool_started" | "tool_finished" }>,
  previous?: ToolRunState,
): ToolRunState {
  return {
    runId: event.tool_run_id,
    toolName: event.tool_name,
    status:
      event.type === "tool_started"
        ? "running"
        : event.status === "failed"
          ? "failed"
          : "succeeded",
    round: event.tool_round,
    inputSummary: event.input_summary ?? previous?.inputSummary ?? null,
    inputParams: event.input_params ?? previous?.inputParams,
    outputSummary:
      event.type === "tool_finished" ? event.output_summary ?? null : previous?.outputSummary,
    artifactRefs:
      event.type === "tool_finished" ? event.artifact_refs ?? previous?.artifactRefs : previous?.artifactRefs,
  };
}

function findRun(
  segments: ChatSegment[],
  runId: string,
): { segmentIndex: number; runIndex: number; run: ToolRunState } | null {
  for (let segmentIndex = 0; segmentIndex < segments.length; segmentIndex += 1) {
    const segment = segments[segmentIndex];
    if (segment.kind !== "tool_group") continue;
    const runIndex = segment.runs.findIndex((run) => run.runId === runId);
    if (runIndex >= 0) {
      return { segmentIndex, runIndex, run: segment.runs[runIndex] };
    }
  }
  return null;
}

function applyToolEvent(
  segments: ChatSegment[],
  event: Extract<TurnStreamEventV3, { type: "tool_started" | "tool_finished" }>,
): ChatSegment[] {
  const found = findRun(segments, event.tool_run_id);
  if (found) {
    const group = segments[found.segmentIndex];
    if (group.kind !== "tool_group") return segments;
    const runs = [...group.runs];
    runs[found.runIndex] = runFromToolEvent(event, found.run);
    return replaceSegment(segments, found.segmentIndex, { ...group, runs });
  }

  const run = runFromToolEvent(event);
  const last = segments.at(-1);
  if (last?.kind === "tool_group" && last.toolRound === event.tool_round) {
    return replaceSegment(segments, segments.length - 1, {
      ...last,
      runs: [...last.runs, run],
    });
  }
  return [
    ...segments,
    {
      kind: "tool_group",
      groupId: `tool-group:${event.tool_run_id}`,
      toolRound: event.tool_round,
      runs: [run],
    },
  ];
}

function applyArtifactEvent(
  segments: ChatSegment[],
  event: Extract<TurnStreamEventV3, { type: "artifact_presented" | "artifact_updated" }>,
): ChatSegment[] {
  const rootArtifactId = event.type === "artifact_updated" ? event.root_artifact_id ?? null : null;
  const artifact = mapStreamUiArtifact(event.artifact, rootArtifactId);
  if (event.type === "artifact_presented") {
    const exists = segments.some(
      (segment) => segment.kind === "artifact" && segment.artifact.artifactId === artifact.artifactId,
    );
    return exists ? segments : [...segments, { kind: "artifact", artifact }];
  }

  const index = segments.findIndex((segment) => {
    if (segment.kind !== "artifact") return false;
    return segment.artifact.artifactId === event.previous_artifact_id
      || (rootArtifactId != null && segment.artifact.rootArtifactId === rootArtifactId);
  });
  return index < 0
    ? [...segments, { kind: "artifact", artifact }]
    : replaceSegment(segments, index, { kind: "artifact", artifact });
}

function compatibilityFields(segments: ChatSegment[]): Pick<
  ChatMessage,
  "content" | "tools" | "toolRuns" | "uiArtifacts"
> {
  const text = segments
    .filter((segment): segment is Extract<ChatSegment, { kind: "text" }> => segment.kind === "text")
    .map((segment) => segment.markdown)
    .filter((markdown) => markdown.length > 0)
    .join("\n\n");
  const toolRuns = segments.flatMap((segment) =>
    segment.kind === "tool_group" ? segment.runs : [],
  );
  const tools = [...new Set(toolRuns.map((run) => run.toolName))];
  const uiArtifacts = segments.flatMap((segment) =>
    segment.kind === "artifact" ? [segment.artifact] : [],
  );
  return {
    content: text,
    tools: tools.length > 0 ? tools : undefined,
    toolRuns: toolRuns.length > 0 ? toolRuns : undefined,
    uiArtifacts: uiArtifacts.length > 0 ? uiArtifacts : undefined,
  };
}

function terminalState(
  message: ChatMessage,
  event: Extract<TurnStreamEventV3, { type: "turn_completed" }>,
): Partial<ChatMessage> {
  const failed = event.outcome === "failed" || event.outcome === "fuse_exhausted";
  return {
    streaming: false,
    answerState: event.outcome,
    failed,
    errorLine: failed ? event.operator_message ?? message.errorLine ?? null : null,
    errorDetail: failed ? event.debug_message ?? message.errorDetail ?? null : null,
  };
}

/** Apply one V3 fact to Medousa's message projection. */
export function applyV3EnvelopeToMessage(
  message: ChatMessage,
  envelope: TurnStreamEnvelopeV3,
): ChatMessage {
  const event = envelope.event;
  const initialSegments = message.segments ?? [];
  let segments = initialSegments;
  let chrome: Partial<ChatMessage> = {};

  switch (event.type) {
    case "assistant_text_started":
      segments = applyTextStarted(segments, event);
      break;
    case "content_append":
      segments = applyContentAppend(segments, event);
      break;
    case "assistant_text_committed":
      segments = applyTextCommitted(segments, event);
      break;
    case "tool_started":
    case "tool_finished":
      segments = applyToolEvent(segments, event);
      break;
    case "artifact_presented":
    case "artifact_updated":
      segments = applyArtifactEvent(segments, event);
      break;
    case "worker_ack":
      segments = [
        ...segments,
        {
          kind: "handoff",
          handoffKind: event.ack_kind,
          text: event.text,
          workId: event.work_id ?? null,
        },
      ];
      break;
    case "reasoning_append":
      chrome = { reasoning: `${message.reasoning ?? ""}${event.text}` };
      break;
    case "status":
      chrome = { phase: event.phase, statusLine: event.operator_message ?? message.statusLine ?? null };
      break;
    case "progress":
      chrome = { statusLine: event.message };
      break;
    case "model_receipt":
      chrome = { responseProvider: event.provider, responseModel: event.model };
      break;
    case "turn_completed":
      chrome = terminalState(message, event);
      break;
    default:
      break;
  }

  const hasSegmentProjection = message.segments !== undefined || segments !== initialSegments;
  return {
    ...message,
    ...chrome,
    ...(hasSegmentProjection ? compatibilityFields(segments) : {}),
    ...(hasSegmentProjection ? { segments } : {}),
  };
}

/** Convenience fold for history/reconnect fixtures; callers choose their facts. */
export function foldV3Envelopes(
  message: ChatMessage,
  envelopes: TurnStreamEnvelopeV3[],
): ChatMessage {
  return envelopes.reduce(applyV3EnvelopeToMessage, message);
}
