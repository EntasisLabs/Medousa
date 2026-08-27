import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type {
  TurnStreamEnvelopeV3,
  TurnStreamEventV3,
} from "$lib/types/generated/daemon_api";

/**
 * Adapter for Home policies that are not transcript projections
 * (permissions, browser control, worker handoffs, and turn ownership).
 * Chronological message rendering consumes V3 directly.
 */
export function v3PresentationEvent(
  envelope: TurnStreamEnvelopeV3,
): InteractiveTurnStreamEvent {
  const base: InteractiveTurnStreamEvent = {
    turn_id: envelope.turn_id,
    seq: envelope.seq,
    event_type: "status",
    phase: "streaming",
    message: "",
    terminal: false,
    emitted_at_utc: envelope.emitted_at_utc,
  };
  return project(base, envelope.event);
}

function project(
  base: InteractiveTurnStreamEvent,
  event: TurnStreamEventV3,
): InteractiveTurnStreamEvent {
  switch (event.type) {
    case "assistant_text_started":
      return base;
    case "content_append":
      return { ...base, event_type: "content_delta", content_delta: event.text };
    case "assistant_text_committed":
      return base;
    case "reasoning_append":
      return { ...base, event_type: "reasoning_delta", reasoning_delta: event.text };
    case "status":
      return {
        ...base,
        phase: event.phase,
        message: event.operator_message ?? event.debug_message ?? "",
        operator_message: event.operator_message,
        debug_message: event.debug_message,
      };
    case "progress":
      return {
        ...base,
        event_type: "turn_progress",
        phase: "tool_loop",
        message: event.message,
        operator_message: event.message,
        tool_names: event.tool_names ?? [],
      };
    case "model_receipt":
      return {
        ...base,
        event_type: "model_receipt",
        phase: "inference",
        message: "Inference route selected",
        response_provider: event.provider,
        response_model: event.model,
      };
    case "worker_ack": {
      const workshop = event.ack_kind === "workshop";
      return {
        ...base,
        event_type: workshop ? "workshop_ack" : "worker_ack",
        phase: workshop ? "workshop_ack" : "worker_ack",
        message: event.text,
        operator_message: event.text,
        final_text: event.text,
        tool_names: event.tool_names ?? [],
        work_id: event.work_id,
      };
    }
    case "worker_synthesis":
      return {
        ...base,
        event_type: "worker_synthesis",
        phase: "worker_synthesis",
        message: event.text,
        final_text: event.text,
        tool_names: event.tool_names ?? [],
        work_id: event.work_id,
        terminal: true,
      };
    case "error":
      return {
        ...base,
        event_type: "error",
        phase: "failed",
        message: event.operator_message,
        operator_message: event.operator_message,
        debug_message: event.debug_message,
      };
    case "tool_started":
      return {
        ...base,
        event_type: "tool_started",
        phase: "tool_loop",
        message: `Running ${event.tool_name}`,
        tool_run_id: event.tool_run_id,
        tool_name: event.tool_name,
        tool_status: "running",
        tool_input_summary: event.input_summary,
        tool_input_params: event.input_params,
        tool_round: event.tool_round,
      };
    case "tool_finished":
      return {
        ...base,
        event_type: "tool_finished",
        phase: "tool_loop",
        message: event.output_summary ?? `${event.tool_name} ${event.status}`,
        tool_run_id: event.tool_run_id,
        tool_name: event.tool_name,
        tool_status: event.status,
        tool_input_summary: event.input_summary,
        tool_input_params: event.input_params,
        tool_output_summary: event.output_summary,
        tool_round: event.tool_round,
        tool_artifact_refs: event.artifact_refs,
      };
    case "artifact_presented":
      return {
        ...base,
        event_type: "artifact_presented",
        phase: "tool_loop",
        message: `Presented ${event.artifact.label}`,
        ui_artifact: event.artifact,
      };
    case "artifact_updated":
      return {
        ...base,
        event_type: "artifact_updated",
        phase: "tool_loop",
        message: `Updated ${event.artifact.label}`,
        ui_artifact: event.artifact,
        previous_artifact_id: event.previous_artifact_id,
        root_artifact_id: event.root_artifact_id,
      };
    case "ui_scene":
      return {
        ...base,
        event_type: "ui_scene",
        phase: "tool_loop",
        message: "Updated the view",
        ui_scene: event.scene,
      };
    case "budget_approval_required":
      return {
        ...base,
        event_type: "budget_approval",
        phase: "awaiting_operator",
        message: event.reason,
        operator_message: event.progress_summary ?? event.reason,
        budget_request_id: event.request_id,
        requested_rounds: event.requested_rounds,
      };
    case "browser_challenge":
      return {
        ...base,
        event_type: "browser_challenge",
        phase: "awaiting_operator",
        message: event.reason,
        browser_session_id: event.session_id,
        browser_challenge_url: event.challenge_url,
      };
    case "browser_navigated":
      return {
        ...base,
        event_type: "browser_navigated",
        phase: "tool",
        message: event.url,
        operator_message: event.title,
      };
    case "context_usage":
      return {
        ...base,
        event_type: "context_usage",
        phase: "orchestration",
        message: event.operator_summary ?? "",
        operator_message: event.operator_summary,
        context_usage: event.report,
      };
    case "permission_request":
      return {
        ...base,
        event_type: "permission_request",
        phase: "awaiting_permission",
        message: event.message,
        permission_request_id: event.request_id,
        agent_session_id: event.agent_session_id,
        agent_runtime: event.agent_runtime,
      };
    case "secret_request":
      return {
        ...base,
        event_type: "secret_request",
        phase: "awaiting_secret",
        message: event.reason,
        secret_request_id: event.request_id,
        secret_label: event.label,
        secret_provider_type: event.provider_type,
        secret_credential_key: event.credential_key,
        secret_backend: event.backend,
        secret_allowed_hosts: event.allowed_hosts,
      };
    case "turn_completed": {
      const failed = event.outcome === "failed" || event.outcome === "fuse_exhausted";
      return {
        ...base,
        event_type: failed ? "error" : "final",
        phase: failed ? "failed" : "complete",
        message: event.operator_message ?? event.aggregate_text,
        operator_message: event.operator_message,
        debug_message: event.debug_message,
        final_text: event.aggregate_text,
        tool_names: event.tool_names ?? [],
        terminal: true,
      };
    }
    default:
      return assertNever(event);
  }
}

function assertNever(value: never): never {
  throw new Error(`unhandled turn stream v3 event: ${JSON.stringify(value)}`);
}
