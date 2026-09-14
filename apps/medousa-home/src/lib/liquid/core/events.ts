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

export type LiquidEventDisposition =
  | "local_state"
  | "context_only"
  | "submit_turn"
  | "navigation"
  | "privileged_action";

export interface SceneEvent {
  nodeId: string;
  type: SceneEventType;
  payload?: unknown;
  ts: number;
  /** Stable child instance inside a node (for example a recipe step timer). */
  instanceId?: string;
  /** Explicit routing; inferred from event type when omitted. */
  disposition?: LiquidEventDisposition;
  /** Component-state revision observed when this event was produced. */
  expectedStateRevision?: number;
}

export interface LiquidInteractionEnvelope {
  version: 1;
  session_id: string;
  message_id: string;
  node_id: string;
  instance_id: string;
  event_type: string;
  disposition: LiquidEventDisposition;
  payload?: unknown;
  occurred_at_utc: string;
  expected_state_revision?: number;
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
