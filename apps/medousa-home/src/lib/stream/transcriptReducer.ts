/**
 * H09 Train 4.1 — typed transcript reducer.
 *
 * `applyStreamEventToMessage` is the focused-field body apply (H03): pure
 * message array in → next messages out. The store writes via `replaceMessageAt`.
 */

import type {
  ChatMessage,
  ContextUsageReport,
  InteractiveTurnStreamEvent,
  ToolRunState,
} from "$lib/types/chat";
import { mapStreamUiArtifact, replaceUiArtifactEntry } from "$lib/types/artifact";
import { resolveTurnContent } from "$lib/utils/resolveTurnContent";
import {
  isEngineTelemetryText,
  operatorStreamStatusLine,
  shouldSuppressStreamContentDelta,
} from "$lib/utils/chatStreamDisplay";
import { stageWhisperAfterFinish, statusLineAfterScratchReset } from "$lib/utils/turnInterimDisplay";
import {
  isBudgetApprovalStreamEvent,
  isBrowserChallengeStreamEvent,
  isPermissionRequestStreamEvent,
  isSecretRequestStreamEvent,
  isTerminalContentCommit,
  isWorkerHandoffStreamEvent,
  isWorkerSynthesisStreamEvent,
  isWorkshopHandoffStreamEvent,
} from "$lib/utils/streamEvents";

/** Terminal answers shorter than this appear instantly. */
const CONTENT_REVEAL_MIN_CHARS = 80;

export type TranscriptReduceContext = {
  messageIdForTurn: (turnId: string) => string | null;
  messageIdForToolStream: (turnId: string) => string | null;
  messageIndexForId: (messageId: string) => number;
  showEngineDetails: boolean;
};

export type StreamMessageApplyCtx = {
  showEngineDetails: boolean;
  /** Worker synthesis should replace (not merge) terminal final_text. */
  replaceFinalText?: boolean;
};

export type StreamMessageFollowUp =
  | "none"
  | "missing"
  | "worker_ack"
  | "workshop_ack"
  | "budget_approval"
  | "terminal"
  | "checkpoint_terminal";

export type StreamMessageApplyResult = {
  messages: ChatMessage[];
  followUp: StreamMessageFollowUp;
  revealContent?: string;
};

export type FocusedStreamRemainder =
  | { type: "none" }
  | { type: "error" }
  | { type: "context_usage"; usage: ContextUsageReport }
  | { type: "worker_synthesis" }
  | { type: "browser_challenge" }
  | { type: "permission_request" }
  | { type: "secret_request" }
  | { type: "permission_resolved" }
  | { type: "browser_navigated" }
  | { type: "ui_scene"; messageId: string }
  | { type: "orphan" }
  | { type: "terminal_empty" }
  | {
      type: "message";
      messageId: string;
      followUp: StreamMessageFollowUp;
      revealContent?: string;
    }
  | { type: "settle_delivered"; messageId: string | null };

export type FocusedStreamApplyCtx = TranscriptReduceContext & {
  workerSynthesisDelivered?: boolean;
  replaceFinalTextFor?: (messageId: string) => boolean;
};

export type FocusedStreamApplyResult = {
  messages: ChatMessage[];
  remainder: FocusedStreamRemainder;
};

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
  if (!showEngineDetails && isEngineTelemetryText(current)) {
    return null;
  }
  return current ?? null;
}

function handoffFollowUp(event: InteractiveTurnStreamEvent): StreamMessageFollowUp {
  if (isWorkerHandoffStreamEvent(event)) return "worker_ack";
  if (isWorkshopHandoffStreamEvent(event)) return "workshop_ack";
  if (isBudgetApprovalStreamEvent(event)) return "budget_approval";
  if (event.terminal) return "terminal";
  return "none";
}

export function applyToolStreamEvent(
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

export function applyArtifactPresented(
  messages: ChatMessage[],
  index: number,
  artifact: NonNullable<InteractiveTurnStreamEvent["ui_artifact"]>,
  rootArtifactId: string | null = null,
): ChatMessage[] | null {
  if (index < 0) return null;
  const current = messages[index];
  const nextArtifact = mapStreamUiArtifact(artifact, rootArtifactId);
  const existing = current.uiArtifacts ?? [];
  if (existing.some((item) => item.artifactId === nextArtifact.artifactId)) {
    return messages;
  }
  return replaceMessage(messages, index, {
    ...current,
    uiArtifacts: [...existing, nextArtifact],
  });
}

export function applyArtifactUpdated(
  messages: ChatMessage[],
  index: number,
  previousArtifactId: string,
  rootArtifactId: string | null,
  artifact: NonNullable<InteractiveTurnStreamEvent["ui_artifact"]>,
): ChatMessage[] | null {
  if (index < 0) return null;
  const current = messages[index];
  const nextArtifact = mapStreamUiArtifact(artifact, rootArtifactId);
  return replaceMessage(messages, index, {
    ...current,
    uiArtifacts: replaceUiArtifactEntry(
      current.uiArtifacts ?? [],
      previousArtifactId,
      rootArtifactId,
      nextArtifact,
    ),
  });
}

/**
 * Pure focused-field body apply. Returns the next messages array; callers write
 * `result.messages[index]` through `replaceMessageAt` (or replace the array).
 */
export function applyStreamEventToMessage(
  messages: ChatMessage[],
  index: number,
  event: InteractiveTurnStreamEvent,
  ctx: StreamMessageApplyCtx,
): StreamMessageApplyResult {
  if (index < 0) {
    return { messages, followUp: "missing" };
  }

  const current = messages[index];
  const showEngineDetails = ctx.showEngineDetails;

  if (event.event_type === "model_receipt") {
    const responseProvider = event.response_provider?.trim();
    const responseModel = event.response_model?.trim();
    if (responseProvider && responseModel) {
      return {
        messages: replaceMessage(messages, index, {
          ...current,
          responseProvider,
          responseModel,
        }),
        followUp: "none",
      };
    }
    return { messages, followUp: "none" };
  }

  if (event.event_type === "assistant_message" && event.final_text?.trim()) {
    return {
      messages: replaceMessage(messages, index, {
        ...current,
        content: event.final_text.trim(),
        phase: null,
        statusLine: null,
      }),
      followUp: "none",
    };
  }

  if (event.event_type === "turn_progress") {
    return {
      messages: replaceMessage(messages, index, {
        ...current,
        phase: "tool_loop",
        statusLine: resolveStatusLine(event, current.statusLine, showEngineDetails),
        tools: event.tool_names?.length
          ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
          : current.tools,
      }),
      followUp: "none",
    };
  }

  if (event.event_type === "assistant_pack_hold") {
    const held = event.final_text?.trim() || event.message?.trim() || current.content;
    return {
      messages: replaceMessage(messages, index, {
        ...current,
        content: held || current.content,
        phase: "pack_hold",
        streaming: true,
        statusLine: event.operator_message?.trim() || "Medousa is finishing this thought…",
        tools: event.tool_names?.length
          ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
          : current.tools,
      }),
      followUp: "none",
    };
  }

  if (event.event_type === "turn_checkpoint") {
    const checkpointBody =
      event.final_text?.trim() || event.message?.trim() || current.content;
    const merged = resolveTurnContent(current.content, checkpointBody, true);
    const next = replaceMessage(messages, index, {
      ...current,
      content: merged,
      phase: "handoff",
      statusLine:
        event.message?.trim() ||
        "Reply when you're ready — Medousa can continue this task.",
      tools: event.tool_names?.length
        ? [...new Set([...(current.tools ?? []), ...event.tool_names])]
        : current.tools,
    });
    return {
      messages: next,
      followUp: event.terminal ? "checkpoint_terminal" : "none",
    };
  }

  if (event.event_type === "scratch_reset") {
    if (current.phase === "pack_hold") {
      return { messages, followUp: "none" };
    }
    return {
      messages: replaceMessage(messages, index, {
        ...current,
        content: "",
        phase: "tool_loop",
        statusLine: statusLineAfterScratchReset(current.content, current.statusLine),
      }),
      followUp: "none",
    };
  }

  let content = current.content;
  if (event.content_delta) {
    if (!shouldSuppressStreamContentDelta(current)) content += event.content_delta;
  } else if (event.final_text) {
    const terminal = isTerminalContentCommit(event);
    content =
      ctx.replaceFinalText && terminal
        ? event.final_text
        : resolveTurnContent(current.content, event.final_text, terminal);

    const shouldReveal =
      event.terminal &&
      terminal &&
      !current.content.trim() &&
      content.trim().length >= CONTENT_REVEAL_MIN_CHARS &&
      !ctx.replaceFinalText;

    if (shouldReveal) {
      let reasoning = current.reasoning ?? "";
      if (event.reasoning_delta) reasoning += event.reasoning_delta;
      const tools = [...(current.tools ?? [])];
      for (const name of event.tool_names ?? []) {
        if (!tools.includes(name)) tools.push(name);
      }
      const next = replaceMessage(messages, index, {
        ...current,
        content: "",
        phase: null,
        statusLine: null,
        stageWhisper: stageWhisperAfterFinish(
          current.statusLine,
          content,
          current.stageWhisper,
        ),
        tools: tools.length > 0 ? tools : current.tools,
        reasoning: reasoning || current.reasoning,
        streaming: false,
      });
      const followUp = handoffFollowUp(event);
      return {
        messages: next,
        followUp: followUp === "terminal" ? "terminal" : followUp,
        revealContent: followUp === "terminal" ? content : undefined,
      };
    }
  }

  let reasoning = current.reasoning ?? "";
  if (event.reasoning_delta) reasoning += event.reasoning_delta;

  const tools = [...(current.tools ?? [])];
  for (const name of event.tool_names ?? []) {
    if (!tools.includes(name)) tools.push(name);
  }

  return {
    messages: replaceMessage(messages, index, {
      ...current,
      content,
      phase: event.phase || current.phase,
      statusLine: resolveStatusLine(event, current.statusLine, showEngineDetails),
      tools: tools.length > 0 ? tools : current.tools,
      reasoning: reasoning || current.reasoning,
    }),
    followUp: handoffFollowUp(event),
  };
}

/**
 * Focused-field apply for internal worker-lane and compatibility events.
 * Mutates messages for tools/artifacts/body; remainder actions stay with the
 * store.
 */
export function applyFocusedStreamEvent(
  messages: ChatMessage[],
  event: InteractiveTurnStreamEvent,
  ctx: FocusedStreamApplyCtx,
): FocusedStreamApplyResult {
  if (event.event_type === "error") {
    return { messages, remainder: { type: "error" } };
  }

  if (event.event_type === "context_usage" && event.context_usage) {
    return {
      messages,
      remainder: { type: "context_usage", usage: event.context_usage },
    };
  }

  if (isWorkerSynthesisStreamEvent(event)) {
    return { messages, remainder: { type: "worker_synthesis" } };
  }

  if (event.terminal && ctx.workerSynthesisDelivered) {
    const messageId = ctx.messageIdForTurn(event.turn_id);
    if (messageId && (event.final_text?.trim() || event.content_delta?.trim())) {
      const applied = applyStreamEventToMessage(
        messages,
        ctx.messageIndexForId(messageId),
        event,
        {
          showEngineDetails: ctx.showEngineDetails,
          replaceFinalText: ctx.replaceFinalTextFor?.(messageId) ?? false,
        },
      );
      return {
        messages: applied.messages,
        remainder: { type: "settle_delivered", messageId },
      };
    }
    return { messages, remainder: { type: "settle_delivered", messageId: null } };
  }

  if (event.event_type === "tool_started" || event.event_type === "tool_finished") {
    const messageId = ctx.messageIdForToolStream(event.turn_id);
    if (!messageId) return { messages, remainder: { type: "none" } };
    const next = applyToolStreamEvent(messages, ctx.messageIndexForId(messageId), event);
    return { messages: next ?? messages, remainder: { type: "none" } };
  }

  if (event.event_type === "artifact_presented" && event.ui_artifact) {
    const messageId = ctx.messageIdForTurn(event.turn_id);
    if (!messageId) return { messages, remainder: { type: "none" } };
    const next = applyArtifactPresented(
      messages,
      ctx.messageIndexForId(messageId),
      event.ui_artifact,
      event.root_artifact_id ?? null,
    );
    return { messages: next ?? messages, remainder: { type: "none" } };
  }

  if (
    event.event_type === "artifact_updated" &&
    event.ui_artifact &&
    event.previous_artifact_id
  ) {
    const messageId = ctx.messageIdForTurn(event.turn_id);
    if (!messageId) return { messages, remainder: { type: "none" } };
    const next = applyArtifactUpdated(
      messages,
      ctx.messageIndexForId(messageId),
      event.previous_artifact_id,
      event.root_artifact_id ?? null,
      event.ui_artifact,
    );
    return { messages: next ?? messages, remainder: { type: "none" } };
  }

  if (event.event_type === "ui_scene") {
    const messageId = ctx.messageIdForTurn(event.turn_id);
    if (messageId && event.ui_scene) {
      return { messages, remainder: { type: "ui_scene", messageId } };
    }
    return { messages, remainder: { type: "none" } };
  }

  if (isBrowserChallengeStreamEvent(event)) {
    return { messages, remainder: { type: "browser_challenge" } };
  }

  if (isPermissionRequestStreamEvent(event)) {
    return { messages, remainder: { type: "permission_request" } };
  }

  if (isSecretRequestStreamEvent(event)) {
    return { messages, remainder: { type: "secret_request" } };
  }

  if (event.event_type === "browser_navigated") {
    return { messages, remainder: { type: "browser_navigated" } };
  }

  const messageId = ctx.messageIdForTurn(event.turn_id);
  if (messageId) {
    const applied = applyStreamEventToMessage(
      messages,
      ctx.messageIndexForId(messageId),
      event,
      {
        showEngineDetails: ctx.showEngineDetails,
        replaceFinalText: ctx.replaceFinalTextFor?.(messageId) ?? false,
      },
    );
    return {
      messages: applied.messages,
      remainder: {
        type: "message",
        messageId,
        followUp: applied.followUp,
        revealContent: applied.revealContent,
      },
    };
  }

  if (event.content_delta || event.final_text || event.event_type === "content_delta") {
    return { messages, remainder: { type: "orphan" } };
  }

  if (event.terminal) {
    return { messages, remainder: { type: "terminal_empty" } };
  }

  return { messages, remainder: { type: "none" } };
}
