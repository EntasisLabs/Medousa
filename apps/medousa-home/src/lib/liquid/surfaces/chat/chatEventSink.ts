/**
 * Chat scene event sink — the return path of the bidirectional loop.
 *
 * Routes user interactions from a rendered scene:
 *   - `submit` / `run` with a submit|prompt action -> a real new chat turn
 *   - `run` with `retry_worker` -> retry the background worker (legacy behavior)
 *   - everything else -> recorded into the interaction buffer for the daemon
 *
 * The routing (`intentFromEvent`, `createChatEventSink`) is pure so it can be
 * unit-tested without Svelte or a live daemon.
 */

import type { SceneEvent } from "$lib/liquid/core";
import type { EventSink } from "$lib/liquid/ports";

/** Actions on a `run` event that mean "start a new turn with this text". */
const SUBMIT_ACTIONS = new Set(["submit", "prompt"]);

function cleanText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/**
 * The turn prompt implied by an event, or null when it should not spawn a turn.
 * `submit` (action_row) carries `intent`; `run` (button) carries an `action` and
 * optional `prompt`/`text`/`intent`.
 */
export function intentFromEvent(event: SceneEvent): string | null {
  const payload = (event.payload ?? {}) as Record<string, unknown>;
  if (event.type === "submit") {
    return cleanText(payload.intent);
  }
  if (event.type === "run") {
    const action = typeof payload.action === "string" ? payload.action : "";
    if (!SUBMIT_ACTIONS.has(action)) return null;
    return cleanText(payload.prompt) ?? cleanText(payload.text) ?? cleanText(payload.intent);
  }
  return null;
}

export interface ChatEventSinkOptions {
  sessionId: string;
  messageId: string;
  /** Start a new interactive turn with the given prompt. */
  onSubmitIntent?: (text: string) => void;
  /** Retry a background worker synthesis (legacy `run`+`retry_worker`). */
  onRetryWorker?: (workId: string) => void;
  /** Capture non-turn events for the model (interaction buffer). */
  record?: (sessionId: string, messageId: string, event: SceneEvent) => void;
}

/**
 * Build the chat surface's event sink. Every event is recorded; turn-spawning
 * and worker-retry are dispatched on top.
 */
export function createChatEventSink(options: ChatEventSinkOptions): EventSink {
  const { sessionId, messageId, onSubmitIntent, onRetryWorker, record } = options;
  return {
    emit(event: SceneEvent) {
      record?.(sessionId, messageId, event);

      if (event.type === "run") {
        const payload = (event.payload ?? {}) as { action?: string; workId?: string };
        if (payload.action === "retry_worker" && payload.workId) {
          onRetryWorker?.(payload.workId);
          return;
        }
      }

      const intent = intentFromEvent(event);
      if (intent) {
        onSubmitIntent?.(intent);
      }
    },
  };
}
