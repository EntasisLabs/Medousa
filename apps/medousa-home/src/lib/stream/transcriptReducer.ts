/**
 * H09 Train 4.1 — typed transcript reducer.
 *
 * StreamEventPump coalesces v2 envelopes; this module is the only owner of
 * v2→legacy interpretation for chat transcript mutations. Side-effecting
 * events (permissions, workers, UI scenes) return `handled: false` so the
 * store can run them without importing v2ToLegacy.
 */

import type {
  ChatMessage,
  ContextUsageReport,
  InteractiveTurnStreamEvent,
  ToolRunState,
} from "$lib/types/chat";
import type { TurnStreamEnvelopeV2 } from "$lib/types/generated/daemon_api";
import { mapStreamUiArtifact, replaceUiArtifactEntry } from "$lib/types/artifact";
import { turnStreamV2ToLegacy } from "$lib/stream/v2ToLegacy";
import { resolveTurnContent } from "$lib/utils/resolveTurnContent";
import {
  operatorStreamStatusLine,
  shouldSuppressStreamContentDelta,
} from "$lib/utils/chatStreamDisplay";
import { statusLineAfterScratchReset } from "$lib/utils/turnInterimDisplay";
import {
  isBudgetApprovalStreamEvent,
  isWorkerHandoffStreamEvent,
  isWorkshopHandoffStreamEvent,
} from "$lib/utils/streamEvents";

export type TranscriptReduceContext = {
  messageIdForTurn: (turnId: string) => string | null;
  messageIdForToolStream: (turnId: string) => string | null;
  messageIndexForId: (messageId: string) => number;
  showEngineDetails: boolean;
};

export type TranscriptReduceResult = {
  legacy: InteractiveTurnStreamEvent;
  handled: boolean;
  messages: ChatMessage[];
  contextUsage?: ContextUsageReport;
};

export function transcriptLegacyFromV2(
  envelope: TurnStreamEnvelopeV2,
): InteractiveTurnStreamEvent {
  return turnStreamV2ToLegacy(envelope);
}

function replaceMessage(
  messages: ChatMessage[],
  index: number,
  next: ChatMessage,
): ChatMessage[] {
  const updated = [...messages];
  updated[index] = next;
  return updated;
}

function resolveStatusLine(
  event: InteractiveTurnStreamEvent,
  current: string | null | undefined,
  showEngineDetails: boolean,
): string | null {
  if (event.message?.trim()) {
    return operatorStreamStatusLine(event, showEngineDetails);
  }
  return current ?? null;
}

function applyToolEvent(
  messages: ChatMessage[],
  index: number,
  event: InteractiveTurnStreamEvent,
): ChatMessage[] | null {
  if (index < 0) return null;
  const runId = event.tool_run_id?.trim();
  const toolName = event.tool_name?.trim();
  if (!runId || !toolName) return null;

  const current = messages[index];
  const runs = [...(current.toolRuns ?? [])];
  const existingIdx = runs.findIndex((run) => run.runId === runId);
  const round = event.tool_round ?? 1;

  if (event.event_type === "tool_started") {
    const next: ToolRunState = {
      runId,
      toolName,
      status: "running",
      round,
      inputSummary: event.tool_input_summary ?? null,
      inputParams: event.tool_input_params ?? undefined,
    };
    if (existingIdx >= 0) runs[existingIdx] = { ...runs[existingIdx], ...next };
    else runs.push(next);
  } else {
    const status: ToolRunState["status"] =
      event.tool_status === "failed" ? "failed" : "succeeded";
    const next: ToolRunState = {
      runId,
      toolName,
      status,
      round,
      inputSummary: event.tool_input_summary ?? runs[existingIdx]?.inputSummary ?? null,
      inputParams: event.tool_input_params ?? runs[existingIdx]?.inputParams ?? undefined,
      outputSummary: event.tool_output_summary ?? null,
      artifactRefs: event.tool_artifact_refs ?? undefined,
    };
    if (existingIdx >= 0) runs[existingIdx] = { ...runs[existingIdx], ...next };
    else runs.push(next);
  }
  runs.sort((a, b) => a.round - b.round || a.toolName.localeCompare(b.toolName));
  const tools = [...(current.tools ?? [])];
  if (!tools.includes(toolName)) tools.push(toolName);
  return replaceMessage(messages, index, {
    ...current,
    toolRuns: runs,
    tools: tools.length > 0 ? tools : current.tools,
  });
}

function applyMessageBody(
  messages: ChatMessage[],
  index: number,
  event: InteractiveTurnStreamEvent,
  showEngineDetails: boolean,
): ChatMessage[] | null {
  if (index < 0) return null;
  const current = messages[index];

  if (event.event_type === "model_receipt") {
    const responseProvider = event.response_provider?.trim();
    const responseModel = event.response_model?.trim();
    if (responseProvider && responseModel) {
      return replaceMessage(messages, index, { ...current, responseProvider, responseModel });
    }
    return messages;
  }

  if (event.event_type === "assistant_message" && event.final_text?.trim()) {
    return replaceMessage(messages, index, {
      ...current,
      content: event.final_text.trim(),
      phase: null,
      statusLine: null,
    });
  }

  if (event.event_type === "turn_progress") {
    return replaceMessage(messages, index, {
      ...current,
      phase: "tool_loop",
      statusLine: resolveStatusLine(event, current.statusLine, showEngineDetails),
      tools: event.tool_names?.length
        ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
        : current.tools,
    });
  }

  if (event.event_type === "assistant_pack_hold") {
    const held = event.final_text?.trim() || event.message?.trim() || current.content;
    return replaceMessage(messages, index, {
      ...current,
      content: held || current.content,
      phase: "pack_hold",
      streaming: true,
      statusLine: event.operator_message?.trim() || "Medousa is finishing this thought…",
      tools: event.tool_names?.length
        ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
        : current.tools,
    });
  }

  if (event.event_type === "scratch_reset") {
    if (current.phase === "pack_hold") return messages;
    return replaceMessage(messages, index, {
      ...current,
      content: "",
      phase: "tool_loop",
      statusLine: statusLineAfterScratchReset(current.content, current.statusLine),
    });
  }

  let content = current.content;
  if (event.content_delta) {
    if (!shouldSuppressStreamContentDelta(current)) content += event.content_delta;
  } else if (event.final_text) {
    content = resolveTurnContent(current.content, event.final_text, false);
  }

  let reasoning = current.reasoning ?? "";
  if (event.reasoning_delta) reasoning += event.reasoning_delta;

  const tools = [...(current.tools ?? [])];
  for (const name of event.tool_names ?? []) {
    if (!tools.includes(name)) tools.push(name);
  }

  return replaceMessage(messages, index, {
    ...current,
    content,
    phase: event.phase || current.phase,
    statusLine: resolveStatusLine(event, current.statusLine, showEngineDetails),
    tools: tools.length > 0 ? tools : current.tools,
    reasoning: reasoning || current.reasoning,
  });
}

/**
 * Apply a v2 envelope to transcript messages when the event is message-only.
 * Returns `handled: false` for worker/permission/terminal/orphan paths.
 */
export function reduceTranscriptEnvelope(
  messages: ChatMessage[],
  envelope: TurnStreamEnvelopeV2,
  ctx: TranscriptReduceContext,
): TranscriptReduceResult {
  const legacy = transcriptLegacyFromV2(envelope);

  if (legacy.event_type === "context_usage" && legacy.context_usage) {
    return {
      legacy,
      handled: true,
      messages,
      contextUsage: legacy.context_usage,
    };
  }

  if (
    legacy.terminal ||
    isWorkerHandoffStreamEvent(legacy) ||
    isWorkshopHandoffStreamEvent(legacy) ||
    isBudgetApprovalStreamEvent(legacy) ||
    legacy.event_type === "error" ||
    legacy.event_type === "ui_scene" ||
    legacy.event_type === "turn_checkpoint" ||
    legacy.event_type === "permission_request" ||
    legacy.event_type === "browser_challenge" ||
    legacy.event_type === "browser_navigated"
  ) {
    return { legacy, handled: false, messages };
  }

  if (legacy.event_type === "tool_started" || legacy.event_type === "tool_finished") {
    const messageId = ctx.messageIdForToolStream(legacy.turn_id);
    if (!messageId) return { legacy, handled: false, messages };
    const next = applyToolEvent(messages, ctx.messageIndexForId(messageId), legacy);
    return next
      ? { legacy, handled: true, messages: next }
      : { legacy, handled: false, messages };
  }

  if (legacy.event_type === "artifact_presented" && legacy.ui_artifact) {
    const messageId = ctx.messageIdForTurn(legacy.turn_id);
    if (!messageId) return { legacy, handled: false, messages };
    const idx = ctx.messageIndexForId(messageId);
    if (idx < 0) return { legacy, handled: false, messages };
    const current = messages[idx];
    const nextArtifact = mapStreamUiArtifact(
      legacy.ui_artifact,
      legacy.root_artifact_id ?? null,
    );
    const existing = current.uiArtifacts ?? [];
    if (existing.some((item) => item.artifactId === nextArtifact.artifactId)) {
      return { legacy, handled: true, messages };
    }
    return {
      legacy,
      handled: true,
      messages: replaceMessage(messages, idx, {
        ...current,
        uiArtifacts: [...existing, nextArtifact],
      }),
    };
  }

  if (
    legacy.event_type === "artifact_updated" &&
    legacy.ui_artifact &&
    legacy.previous_artifact_id
  ) {
    const messageId = ctx.messageIdForTurn(legacy.turn_id);
    if (!messageId) return { legacy, handled: false, messages };
    const idx = ctx.messageIndexForId(messageId);
    if (idx < 0) return { legacy, handled: false, messages };
    const current = messages[idx];
    const nextArtifact = mapStreamUiArtifact(
      legacy.ui_artifact,
      legacy.root_artifact_id ?? null,
    );
    return {
      legacy,
      handled: true,
      messages: replaceMessage(messages, idx, {
        ...current,
        uiArtifacts: replaceUiArtifactEntry(
          current.uiArtifacts ?? [],
          legacy.previous_artifact_id,
          legacy.root_artifact_id ?? null,
          nextArtifact,
        ),
      }),
    };
  }

  const messageId = ctx.messageIdForTurn(legacy.turn_id);
  if (!messageId) return { legacy, handled: false, messages };

  if (
    legacy.event_type === "content_delta" ||
    legacy.event_type === "reasoning_delta" ||
    legacy.event_type === "model_receipt" ||
    legacy.event_type === "assistant_message" ||
    legacy.event_type === "turn_progress" ||
    legacy.event_type === "assistant_pack_hold" ||
    legacy.event_type === "scratch_reset"
  ) {
    const next = applyMessageBody(
      messages,
      ctx.messageIndexForId(messageId),
      legacy,
      ctx.showEngineDetails,
    );
    return next
      ? { legacy, handled: true, messages: next }
      : { legacy, handled: false, messages };
  }

  return { legacy, handled: false, messages };
}
