/**
 * Chat surface adapter — maps a `ChatMessage` to a runtime-governed `document`.
 *
 * Order (alive, not buried): thinking → live pulse → settled whisper → body → tools.
 * Intensity is dialed down in the shell components; we do not hide receipts
 * behind an observability drawer.
 */

import { createNode, type SceneNode } from "$lib/liquid/core";
import type { ChatMessage, ToolRunState } from "$lib/types/chat";
import { stripChatBodyChrome } from "./stripChatBodyChrome";

export interface ChatSceneOptions {
  /** Visible status line, already resolved against the engine-details setting. */
  statusLine?: string | null;
  /** Render the status pill in a warn tone (worker handoff / awaiting operator). */
  statusWarn?: boolean;
}

function child(
  id: string,
  type: string,
  props: Record<string, unknown>,
  slots?: Record<string, SceneNode[]>,
): SceneNode {
  return createNode({ id, type, props, slots, fillState: "ready" });
}

function livePulseLabel(
  message: ChatMessage,
  opts: ChatSceneOptions,
  hasReasoning: boolean,
): string | null {
  if (!message.streaming) return null;
  if (opts.statusLine?.trim()) return opts.statusLine.trim();
  if (message.stageWhisper?.trim()) return message.stageWhisper.trim();
  // Reasoning already paints as the thinking shell — don't double-label.
  if (hasReasoning) return null;
  const hasContent = Boolean(message.content?.trim());
  const hasTools = Boolean(message.toolRuns?.length || message.tools?.length);
  if (!hasContent && !hasTools) return "Thinking…";
  return null;
}

/** Settled interim above the final answer (live finish whisper or history Progress). */
function settledInterimLabel(
  message: ChatMessage,
  opts: ChatSceneOptions,
  bodyMarkdown: string,
): string | null {
  if (message.streaming) return null;
  const whisper =
    message.stageWhisper?.trim() || opts.statusLine?.trim() || null;
  if (!whisper) return null;
  const body = bodyMarkdown.trim();
  if (body && whisper === body) return null;
  return whisper;
}

function assistantFlow(message: ChatMessage, opts: ChatSceneOptions): SceneNode[] {
  const flow: SceneNode[] = [];
  const id = message.id;
  const streaming = Boolean(message.streaming);
  const stripped = stripChatBodyChrome(message.content ?? "");
  const bodyMarkdown = stripped.markdown;
  const hasContent = Boolean(bodyMarkdown.trim());
  const reasoningText =
    [message.reasoning?.trim(), stripped.recoveredReasoning?.trim()]
      .filter(Boolean)
      .join("\n\n") || null;
  const hasReasoning = Boolean(reasoningText);
  const toolRuns =
    message.toolRuns && message.toolRuns.length > 0
      ? message.toolRuns
      : toolRunsFromNames(message.tools);
  const hasToolRuns = toolRuns.length > 0;

  // 1. Thinking on top (collapsed when done — soft chrome, not a drawer)
  if (hasReasoning) {
    flow.push(
      child(`${id}:thinking`, "thinking", {
        reasoning: reasoningText,
        streaming,
      }),
    );
  }

  // 2. Live pulse — quiet status while streaming (never duplicates thinking)
  const pulse = livePulseLabel(message, opts, hasReasoning);
  if (pulse) {
    flow.push(
      child(`${id}:pulse`, "status_pill", {
        label: pulse,
        state: opts.statusWarn ? "warn" : "loading",
        quiet: true,
      }),
    );
  }

  // 3. Errors before body
  if (message.failed && message.errorLine) {
    flow.push(child(`${id}:error`, "callout", { tone: "error", body: message.errorLine }));
    if (message.workId) {
      flow.push(
        child(`${id}:retry`, "button", {
          label: "Retry",
          action: "retry_worker",
          payload: { workId: message.workId },
        }),
      );
    }
  }

  // 4. Settled interim whisper above the final answer (tool-turn progress breadcrumb)
  const settled = settledInterimLabel(message, opts, bodyMarkdown);
  if (settled) {
    flow.push(child(`${id}:whisper`, "whisper", { text: settled }));
  }

  // 5. Body (substance) — never paint leaked reasoning callouts in prose
  if (hasContent) {
    flow.push(child(`${id}:body`, "prose", { markdown: bodyMarkdown }));
  } else if (streaming && !hasToolRuns && !hasReasoning) {
    flow.push(child(`${id}:body`, "prose", { markdown: "…" }));
  }

  if (message.uiArtifacts && message.uiArtifacts.length > 0) {
    flow.push(child(`${id}:artifacts`, "presentation", { artifacts: message.uiArtifacts }));
  }

  // 6. Tool receipts at the bottom — host-lane ToolRunChips (footnote when settled)
  if (hasToolRuns) {
    flow.push(
      child(`${id}:tools`, "tool_trace", {
        runs: toolRuns,
        turnIndex: message.turnIndex ?? null,
        streaming,
        compact: true,
      }),
    );
  }

  return flow;
}

/** Workshop/worker turns often only ship tool names — promote to host lineage UI. */
function toolRunsFromNames(tools: string[] | undefined): ToolRunState[] {
  if (!tools?.length) return [];
  return tools.map((toolName, index) => ({
    runId: `named:${index}:${toolName}`,
    toolName,
    status: "succeeded" as const,
    round: index + 1,
  }));
}

function plainFlow(message: ChatMessage): SceneNode[] {
  const flow: SceneNode[] = [];
  const id = message.id;
  if (message.content?.trim()) {
    flow.push(child(`${id}:body`, "prose", { markdown: message.content, plain: true }));
  }
  const attachments = message.mediaAttachments ?? [];
  if (attachments.length > 0) {
    flow.push(child(`${id}:media`, "chat_media", { attachments }));
  }
  return flow;
}

/** Build the `document` scene node for a single chat message. */
export function chatMessageToScene(message: ChatMessage, opts: ChatSceneOptions = {}): SceneNode {
  const flow =
    message.role === "assistant" ? assistantFlow(message, opts) : plainFlow(message);
  return createNode({
    id: `${message.id}:doc`,
    type: "document",
    props: {},
    slots: { flow },
    fillState: "ready",
  });
}
