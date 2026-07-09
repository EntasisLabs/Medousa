/**
 * Chat surface adapter — maps a `ChatMessage` to a `document` scene (no daemon).
 *
 * Phase 1 of the daemon seam: existing turns become scenes with zero engine
 * change, so the scene renderer + reuse archetypes light up immediately. Node
 * ids are derived from the message id so reconciliation preserves each part's
 * instance across streaming updates.
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
): SceneNode {
  return createNode({ id, type, props, fillState: "ready" });
}

function assistantFlow(message: ChatMessage, opts: ChatSceneOptions): SceneNode[] {
  const flow: SceneNode[] = [];
  const id = message.id;
  const streaming = Boolean(message.streaming);
  const hasContent = Boolean(message.content?.trim());
  const hasWhisper = Boolean(message.stageWhisper?.trim());
  const hasReasoning = Boolean(message.reasoning?.trim());

  if (hasWhisper) {
    flow.push(child(`${id}:whisper`, "whisper", { text: message.stageWhisper }));
  }

  if (hasReasoning) {
    flow.push(child(`${id}:thinking`, "thinking", { reasoning: message.reasoning, streaming }));
  } else if (streaming && !hasContent && !hasWhisper) {
    flow.push(child(`${id}:thinking-pill`, "status_pill", { label: "Thinking…", state: "loading" }));
  }

  if (opts.statusLine && streaming) {
    flow.push(
      child(`${id}:status`, "status_pill", {
        label: opts.statusLine,
        state: opts.statusWarn ? "warn" : "loading",
      }),
    );
  }

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

  if (hasContent) {
    flow.push(child(`${id}:body`, "prose", { markdown: message.content }));
  } else if (streaming && !message.toolRuns?.length) {
    flow.push(child(`${id}:body`, "prose", { markdown: "…" }));
  }

  if (message.toolRuns && message.toolRuns.length > 0) {
    flow.push(
      child(`${id}:tools`, "tool_trace", {
        runs: message.toolRuns,
        turnIndex: message.turnIndex ?? null,
        streaming,
      }),
    );
  } else if (message.tools && message.tools.length > 0) {
    flow.push(child(`${id}:tools`, "metadata", { parts: message.tools.map(formatToolName) }));
  }

  if (message.uiArtifacts && message.uiArtifacts.length > 0) {
    flow.push(child(`${id}:artifacts`, "presentation", { artifacts: message.uiArtifacts }));
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
