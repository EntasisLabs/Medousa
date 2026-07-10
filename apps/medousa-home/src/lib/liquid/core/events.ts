/**
 * Liquid UI — scene events (events up).
 *
 * User interaction on a rendered node emits a structured `SceneEvent`. These
 * flow back into the model's context (context path) and can trigger binding
 * writes directly (fast path). Pure types only.
 */

export type SceneEventType =
  | "select"
  | "expand"
  | "collapse"
  | "filter"
  | "sort"
  | "edit"
  | "submit"
  | "run"
  | "pin"
  | "reorder"
  | "navigate"
  | "focus"
  | "dismiss"
  | "scroll_end";

export interface SceneEvent {
  nodeId: string;
  type: SceneEventType;
  payload?: unknown;
  ts: number;
}

/** Build a scene event, defaulting the timestamp to now. */
export function createSceneEvent(
  nodeId: string,
  type: SceneEventType,
  payload?: unknown,
  ts: number = Date.now(),
): SceneEvent {
  const event: SceneEvent = { nodeId, type, ts };
  if (payload !== undefined) event.payload = payload;
  return event;
}
