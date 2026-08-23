import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type {
  TurnStreamEnvelopeV2,
  TurnStreamEventV2,
} from "$lib/types/generated/daemon_api";
import { isTurnStreamEnvelopeV2 } from "$lib/stream/v2";

/**
 * The single Home compatibility seam while the mature chat policies still
 * consume v1. The switch is exhaustive over the schema-generated v2 union.
 */
export function turnStreamV2ToLegacy(
  envelope: TurnStreamEnvelopeV2,
): InteractiveTurnStreamEvent {
  const base: InteractiveTurnStreamEvent = {
    turn_id: envelope.turn_id,
    seq: envelope.seq,
    event_type: "status",
    phase: "starting",
    message: "",
    terminal: false,
    emitted_at_utc: envelope.emitted_at_utc,
  };
  return projectEvent(base, envelope.event);
}

export function turnStreamPayloadToLegacy(
  payload: TurnStreamEnvelopeV2 | InteractiveTurnStreamEvent,
): InteractiveTurnStreamEvent {
  return isTurnStreamEnvelopeV2(payload) ? turnStreamV2ToLegacy(payload) : payload;
}

/**
 * Cold compatibility path for workshops that predate stream v2 negotiation.
 * Current Home/daemon traffic is already v2 and returns without allocation.
 */
export function turnStreamPayloadToV2(
  payload: TurnStreamEnvelopeV2 | InteractiveTurnStreamEvent,
): TurnStreamEnvelopeV2 {
  if (isTurnStreamEnvelopeV2(payload)) return payload;
  return {
    schema_version: 2,
    turn_id: payload.turn_id,
    seq: payload.seq ?? 0,
    emitted_at_utc: payload.emitted_at_utc,
    event: legacyEventToV2(payload),
  };
}

function legacyEventToV2(event: InteractiveTurnStreamEvent): TurnStreamEventV2 {
  const text = event.final_text ?? event.message;
  const toolNames = event.tool_names ?? [];
  switch (event.event_type) {
    case "content_delta":
      return { type: "content_append", text: event.content_delta ?? "" };
    case "reasoning_delta":
      return { type: "reasoning_append", text: event.reasoning_delta ?? "" };
    case "status":
      return {
        type: "status",
        phase: event.phase,
        operator_message: event.operator_message,
        debug_message: event.debug_message,
      };
    case "turn_progress":
      return { type: "progress", message: event.message, tool_names: toolNames };
    case "assistant_pack_hold":
      return { type: "pack_hold", text, tool_names: toolNames };
    case "model_receipt":
      return {
        type: "model_receipt",
        provider: event.response_provider ?? "",
        model: event.response_model ?? "",
      };
    case "final":
      return { type: "final", text, tool_names: toolNames };
    case "needs_input":
      return { type: "needs_input", text, tool_names: toolNames };
    case "checkpoint":
    case "turn_checkpoint":
      return { type: "checkpoint", text, tool_names: toolNames };
    case "worker_synthesis":
      return { type: "worker_synthesis", text, tool_names: toolNames, work_id: event.work_id };
    case "worker_ack":
    case "workshop_ack":
      return {
        type: "worker_ack",
        ack_kind: event.event_type === "workshop_ack" ? "workshop" : "worker",
        text,
        tool_names: toolNames,
        work_id: event.work_id,
      };
    case "final_pending":
      return { type: "final_pending", text, tool_names: toolNames };
    case "error":
      return {
        type: "error",
        operator_message: event.operator_message ?? event.message,
        debug_message: event.debug_message,
      };
    case "scratch_reset":
      return { type: "scratch_reset" };
    case "tool_started":
      return {
        type: "tool_started",
        tool_run_id: requiredLegacy(event.tool_run_id, "tool_run_id"),
        tool_name: requiredLegacy(event.tool_name, "tool_name"),
        input_summary: requiredLegacy(event.tool_input_summary, "tool_input_summary"),
        input_params: event.tool_input_params ?? [],
        tool_round: event.tool_round ?? 1,
      };
    case "tool_finished":
      return {
        type: "tool_finished",
        tool_run_id: requiredLegacy(event.tool_run_id, "tool_run_id"),
        tool_name: requiredLegacy(event.tool_name, "tool_name"),
        status: requiredLegacy(event.tool_status, "tool_status"),
        input_summary: event.tool_input_summary ?? "",
        input_params: event.tool_input_params ?? [],
        output_summary: event.tool_output_summary,
        tool_round: event.tool_round ?? 1,
        artifact_refs: event.tool_artifact_refs ?? [],
      };
    case "artifact_presented":
      return {
        type: "artifact_presented",
        artifact: requiredLegacy(event.ui_artifact, "ui_artifact"),
      };
    case "artifact_updated":
      return {
        type: "artifact_updated",
        previous_artifact_id: requiredLegacy(
          event.previous_artifact_id,
          "previous_artifact_id",
        ),
        artifact: requiredLegacy(event.ui_artifact, "ui_artifact"),
        root_artifact_id: event.root_artifact_id,
      };
    case "ui_scene":
      return { type: "ui_scene", scene: requiredLegacy(event.ui_scene, "ui_scene") };
    case "budget_approval":
      return {
        type: "budget_approval_required",
        request_id: requiredLegacy(event.budget_request_id, "budget_request_id"),
        rounds_executed: 0,
        max_tool_rounds: 0,
        requested_rounds: event.requested_rounds ?? 0,
        reason: event.message,
        progress_summary: event.operator_message,
      };
    case "browser_challenge":
      return {
        type: "browser_challenge",
        session_id: requiredLegacy(event.browser_session_id, "browser_session_id"),
        challenge_url: requiredLegacy(event.browser_challenge_url, "browser_challenge_url"),
        reason: event.message,
      };
    case "browser_navigated":
      return {
        type: "browser_navigated",
        url: event.message,
        title: event.operator_message,
        opened_by_agent: false,
      };
    case "context_usage":
      return {
        type: "context_usage",
        report: requiredLegacy(event.context_usage, "context_usage"),
        operator_summary: event.operator_message,
      };
    case "permission_request":
      return {
        type: "permission_request",
        request_id: requiredLegacy(event.permission_request_id, "permission_request_id"),
        message: event.message,
        agent_session_id: event.agent_session_id,
        agent_runtime: event.agent_runtime,
      };
    case "secret_request":
      return {
        type: "secret_request",
        request_id: requiredLegacy(event.secret_request_id, "secret_request_id"),
        label: requiredLegacy(event.secret_label, "secret_label"),
        reason: event.message,
        provider_type: requiredLegacy(event.secret_provider_type, "secret_provider_type"),
        credential_key: requiredLegacy(event.secret_credential_key, "secret_credential_key"),
        backend: event.secret_backend ?? "openshell_provider",
        allowed_hosts: event.secret_allowed_hosts ?? [],
      };
    default:
      throw new Error(`legacy stream event '${event.event_type}' has no v2 projection`);
  }
}

function requiredLegacy<T>(value: T | null | undefined, field: string): T {
  if (value == null) throw new Error(`legacy stream event is missing ${field}`);
  return value;
}

function projectEvent(
  base: InteractiveTurnStreamEvent,
  event: TurnStreamEventV2,
): InteractiveTurnStreamEvent {
  switch (event.type) {
    case "content_append":
      return { ...base, event_type: "content_delta", phase: "streaming", content_delta: event.text };
    case "reasoning_append":
      return {
        ...base,
        event_type: "reasoning_delta",
        phase: "streaming",
        reasoning_delta: event.text,
      };
    case "status":
      return {
        ...base,
        event_type: "status",
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
    case "pack_hold":
      return {
        ...bodyEvent(base, "assistant_pack_hold", "pack_hold", event.text, event.tool_names, false),
        operator_message: event.text,
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
    case "final":
      return bodyEvent(base, "final", "complete", event.text, event.tool_names, true);
    case "needs_input":
      return bodyEvent(base, "needs_input", "awaiting_operator", event.text, event.tool_names, true);
    case "checkpoint":
      return bodyEvent(base, "turn_checkpoint", "handoff", event.text, event.tool_names, true);
    case "worker_ack": {
      const workshop = event.ack_kind === "workshop";
      const message = workshop ? "bound workshop started" : "background worker started";
      return {
        ...base,
        event_type: workshop ? "workshop_ack" : "worker_ack",
        phase: workshop ? "workshop_ack" : "worker_ack",
        message,
        operator_message: message,
        final_text: event.text,
        tool_names: event.tool_names ?? [],
        work_id: event.work_id,
      };
    }
    case "worker_synthesis":
      return {
        ...bodyEvent(base, "worker_synthesis", "worker_synthesis", event.text, event.tool_names, true),
        work_id: event.work_id,
      };
    case "final_pending":
      return {
        ...bodyEvent(base, "final_pending", "wrapping_up", event.text, event.tool_names, false),
        message: "Medousa is preparing your final answer",
        operator_message: "Medousa is preparing your final answer",
      };
    case "error":
      return {
        ...base,
        event_type: "error",
        phase: "failed",
        message: event.operator_message,
        operator_message: event.operator_message,
        debug_message: event.debug_message,
        terminal: true,
      };
    case "scratch_reset":
      return {
        ...base,
        event_type: "scratch_reset",
        phase: "streaming",
        debug_message: "assistant scratch cleared",
      };
    case "tool_started": {
      const message = `Running ${event.tool_name}`;
      return {
        ...base,
        event_type: "tool_started",
        phase: "tool_loop",
        message,
        operator_message: message,
        tool_run_id: event.tool_run_id,
        tool_name: event.tool_name,
        tool_status: "running",
        tool_input_summary: event.input_summary,
        tool_input_params: event.input_params?.length ? event.input_params : null,
        tool_round: event.tool_round,
      };
    }
    case "tool_finished": {
      const message = event.output_summary
        ? `${event.tool_name}: ${event.output_summary}`
        : `${event.tool_name} ${event.status}`;
      return {
        ...base,
        event_type: "tool_finished",
        phase: "tool_loop",
        message,
        operator_message: message,
        tool_run_id: event.tool_run_id,
        tool_name: event.tool_name,
        tool_status: event.status,
        tool_input_summary: event.input_summary,
        tool_input_params: event.input_params?.length ? event.input_params : null,
        tool_output_summary: event.output_summary,
        tool_round: event.tool_round,
        tool_artifact_refs: event.artifact_refs?.length ? event.artifact_refs : null,
      };
    }
    case "artifact_presented": {
      const message = `Presented ${event.artifact.label}`;
      return {
        ...base,
        event_type: "artifact_presented",
        phase: "tool_loop",
        message,
        operator_message: message,
        ui_artifact: event.artifact,
      };
    }
    case "artifact_updated": {
      const message = `Updated ${event.artifact.label}`;
      return {
        ...base,
        event_type: "artifact_updated",
        phase: "tool_loop",
        message,
        operator_message: message,
        ui_artifact: event.artifact,
        previous_artifact_id: event.previous_artifact_id,
        root_artifact_id: event.root_artifact_id,
      };
    }
    case "ui_scene":
      return {
        ...base,
        event_type: "ui_scene",
        phase: "tool_loop",
        message: "Updated the view",
        operator_message: "Updated the view",
        ui_scene: event.scene,
      };
    case "budget_approval_required": {
      const message = `Turn paused at ${event.rounds_executed}/${event.max_tool_rounds}. Requesting +${event.requested_rounds} rounds: ${event.reason}`;
      return {
        ...base,
        event_type: "budget_approval",
        phase: "awaiting_operator",
        message,
        operator_message: event.progress_summary ?? message,
        budget_request_id: event.request_id,
        requested_rounds: event.requested_rounds,
      };
    }
    case "browser_challenge":
      return {
        ...base,
        event_type: "browser_challenge",
        phase: "awaiting_operator",
        message: event.reason,
        operator_message: event.reason,
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
        operator_message: event.message,
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
        operator_message: event.reason,
        secret_request_id: event.request_id,
        secret_label: event.label,
        secret_provider_type: event.provider_type,
        secret_credential_key: event.credential_key,
        secret_backend: event.backend,
        secret_allowed_hosts: event.allowed_hosts,
      };
    default:
      return assertNever(event);
  }
}

function bodyEvent(
  base: InteractiveTurnStreamEvent,
  eventType: string,
  phase: string,
  text: string,
  toolNames: string[] | undefined,
  terminal: boolean,
): InteractiveTurnStreamEvent {
  return {
    ...base,
    event_type: eventType,
    phase,
    message: text,
    final_text: text,
    tool_names: toolNames ?? [],
    terminal,
  };
}

function assertNever(value: never): never {
  throw new Error(`unhandled turn stream v2 event: ${JSON.stringify(value)}`);
}
