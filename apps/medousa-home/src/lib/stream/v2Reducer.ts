import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";

export interface TurnStreamV2State {
  seq: number;
  content: string;
  reasoning: string;
  phase: string;
  terminal: boolean;
  error: string | null;
  activeToolRuns: string[];
  artifactIds: string[];
}

export function emptyTurnStreamV2State(): TurnStreamV2State {
  return {
    seq: 0,
    content: "",
    reasoning: "",
    phase: "starting",
    terminal: false,
    error: null,
    activeToolRuns: [],
    artifactIds: [],
  };
}

export function reduceTurnStreamV2(
  state: TurnStreamV2State,
  envelope: TurnStreamEnvelopeV2,
): TurnStreamV2State {
  if (envelope.schema_version !== 2 || envelope.seq <= state.seq) return state;
  const next = { ...state, seq: envelope.seq };
  return reduceEvent(next, envelope.event);
}

function reduceEvent(
  state: TurnStreamV2State,
  event: TurnStreamEventV2,
): TurnStreamV2State {
  switch (event.type) {
    case "content_append":
      return { ...state, content: state.content + event.text, phase: "streaming" };
    case "reasoning_append":
      return { ...state, reasoning: state.reasoning + event.text, phase: "streaming" };
    case "status":
      return { ...state, phase: event.phase };
    case "progress":
      return { ...state, phase: "tool_loop" };
    case "pack_hold":
      return { ...state, phase: "pack_hold" };
    case "model_receipt":
      return { ...state, phase: "inference" };
    case "final":
      return { ...state, content: event.text, phase: "complete", terminal: true };
    case "needs_input":
      return { ...state, content: event.text, phase: "awaiting_operator", terminal: true };
    case "checkpoint":
      return { ...state, content: event.text, phase: "handoff", terminal: true };
    case "worker_ack":
      return { ...state, phase: event.ack_kind === "workshop" ? "workshop_ack" : "worker_ack" };
    case "worker_synthesis":
      return { ...state, content: event.text, phase: "worker_synthesis", terminal: true };
    case "final_pending":
      return { ...state, phase: "wrapping_up" };
    case "error":
      return {
        ...state,
        phase: "failed",
        terminal: true,
        error: event.operator_message,
      };
    case "scratch_reset":
      return { ...state, content: "", reasoning: "", phase: "streaming" };
    case "tool_started":
      return {
        ...state,
        phase: "tool_loop",
        activeToolRuns: [...state.activeToolRuns, event.tool_run_id],
      };
    case "tool_finished":
      return {
        ...state,
        phase: "tool_loop",
        activeToolRuns: state.activeToolRuns.filter((id) => id !== event.tool_run_id),
      };
    case "artifact_presented":
      return { ...state, artifactIds: [...state.artifactIds, event.artifact.artifact_id] };
    case "artifact_updated":
      return {
        ...state,
        artifactIds: state.artifactIds.map((id) =>
          id === event.previous_artifact_id ? event.artifact.artifact_id : id,
        ),
      };
    case "ui_scene":
      return state;
    case "budget_approval_required":
      return { ...state, phase: "awaiting_operator" };
    case "browser_challenge":
      return { ...state, phase: "awaiting_operator" };
    case "browser_navigated":
      return { ...state, phase: "tool" };
    case "context_usage":
      return state;
    case "permission_request":
      return { ...state, phase: "awaiting_permission" };
    default:
      return assertNever(event);
  }
}

function assertNever(value: never): never {
  throw new Error(`unhandled turn stream v2 event: ${JSON.stringify(value)}`);
}
