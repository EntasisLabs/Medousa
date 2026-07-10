/**
 * Chat surface adapter — maps a `ChatMessage` to a runtime-governed `document`.
 *
 * Order (alive, not buried): thinking → live pulse → body → tools.
 * Intensity is dialed down in the shell components; we do not hide receipts
 * behind an observability drawer.
 */

import { createNode, type SceneNode } from "$lib/liquid/core";
import type { ChatMessage } from "$lib/types/chat";
import { formatToolName } from "$lib/utils/formatTurn";

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
): string | null {
  if (!message.streaming) return null;
  if (opts.statusLine?.trim()) return opts.statusLine.trim();
  if (message.stageWhisper?.trim()) return message.stageWhisper.trim();
  // Reasoning already paints as the thinking shell — don't double-label.
  if (message.reasoning?.trim()) return null;
  const hasContent = Boolean(message.content?.trim());
  const hasTools = Boolean(message.toolRuns?.length);
  if (!hasContent && !hasTools) return "Thinking…";
  return null;
}

function assistantFlow(message: ChatMessage, opts: ChatSceneOptions): SceneNode[] {
  const flow: SceneNode[] = [];
  const id = message.id;
  const streaming = Boolean(message.streaming);
  const hasContent = Boolean(message.content?.trim());
  const hasReasoning = Boolean(message.reasoning?.trim());
  const toolRuns = message.toolRuns;
  const hasToolRuns = Boolean(toolRuns && toolRuns.length > 0);
  const hasToolNames = Boolean(message.tools && message.tools.length > 0);

  // 1. Thinking on top (collapsed when done — soft chrome, not a drawer)
  if (hasReasoning) {
    flow.push(
      child(`${id}:thinking`, "thinking", {
        reasoning: message.reasoning,
        streaming,
      }),
    );
  }

  // 2. Live pulse — quiet status while streaming (never duplicates thinking)
  const pulse = livePulseLabel(message, opts);
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

  // 4. Body (substance)
  if (hasContent) {
    flow.push(child(`${id}:body`, "prose", { markdown: message.content }));
  } else if (streaming && !hasToolRuns && !hasReasoning) {
    flow.push(child(`${id}:body`, "prose", { markdown: "…" }));
  }

  if (message.uiArtifacts && message.uiArtifacts.length > 0) {
    flow.push(child(`${id}:artifacts`, "presentation", { artifacts: message.uiArtifacts }));
  }

  // 5. Tool receipts at the bottom — compact host-lane footnote when settled
  if (hasToolRuns && toolRuns) {
    flow.push(
      child(`${id}:tools`, "tool_trace", {
        runs: toolRuns,
        turnIndex: message.turnIndex ?? null,
        streaming,
        compact: true,
      }),
    );
  } else if (hasToolNames && message.tools) {
    flow.push(
      child(`${id}:tools`, "metadata", {
        parts: message.tools.map(formatToolName),
      }),
    );
  }

  return flow;
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
