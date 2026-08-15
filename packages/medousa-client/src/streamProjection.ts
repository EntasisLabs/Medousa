import type { TurnStreamEnvelopeV2, TurnStreamEventV2 } from "./types.js";

export type TurnStreamProjectedEvent =
  | { kind: "answer_delta"; text: string }
  | { kind: "answer_replace"; text: string }
  | { kind: "status"; text: string }
  | { kind: "tool_started"; runId: string; name: string; summary?: string }
  | { kind: "tool_finished"; runId: string; name: string; status: string; summary?: string }
  | { kind: "budget_request"; requestId: string; rounds: number }
  | { kind: "permission_request"; requestId: string; message: string }
  | { kind: "handoff"; text: string; workId?: string }
  | { kind: "terminal"; text?: string; error?: boolean };

export interface TurnStreamProjectionState {
  answerText: string;
  toolRuns: Set<string>;
  showEngineDetails: boolean;
}

export function createTurnStreamProjectionState(
  showEngineDetails = false,
): TurnStreamProjectionState {
  return { answerText: "", toolRuns: new Set(), showEngineDetails };
}

export function projectTurnStreamEvent(
  envelope: TurnStreamEnvelopeV2,
  state: TurnStreamProjectionState,
): TurnStreamProjectedEvent[] {
  const event = envelope.event;
  const projected: TurnStreamProjectedEvent[] = [];

  switch (event.type) {
    case "content_append":
      if (state.toolRuns.size === 0 && event.text) {
        state.answerText += event.text;
        projected.push({ kind: "answer_delta", text: event.text });
      }
      break;
    case "reasoning_append":
    case "scratch_reset":
      break;
    case "status":
      pushStatus(projected, event.operator_message);
      if (state.showEngineDetails) pushStatus(projected, event.debug_message);
      break;
    case "progress":
    case "pack_hold":
      pushStatus(projected, "message" in event ? event.message : event.text);
      break;
    case "model_receipt":
      if (state.showEngineDetails) {
        projected.push({ kind: "status", text: `${event.provider} · ${event.model}` });
      }
      break;
    case "final":
    case "needs_input":
    case "checkpoint":
    case "worker_synthesis":
      projectTerminalText(projected, state, event.text);
      projected.push({ kind: "terminal", error: false });
      break;
    case "worker_ack":
      projected.push({
        kind: "handoff",
        text:
          event.ack_kind === "worker" ? "Background work started" : "Medousa is in the workshop",
        workId: event.work_id ?? undefined,
      });
      break;
    case "final_pending":
      projected.push({ kind: "status", text: "Medousa is preparing your final answer" });
      break;
    case "error":
      projected.push({
        kind: "terminal",
        text: friendlyError(event.operator_message),
        error: true,
      });
      break;
    case "tool_started":
      state.toolRuns.add(event.tool_run_id);
      projected.push({
        kind: "tool_started",
        runId: event.tool_run_id,
        name: formatToolName(event.tool_name),
        summary: event.input_summary || undefined,
      });
      break;
    case "tool_finished":
      state.toolRuns.add(event.tool_run_id);
      projected.push({
        kind: "tool_finished",
        runId: event.tool_run_id,
        name: formatToolName(event.tool_name),
        status: event.status,
        summary: event.output_summary ?? undefined,
      });
      break;
    case "artifact_presented":
      projected.push({ kind: "status", text: `Presented ${event.artifact.label}` });
      break;
    case "artifact_updated":
      projected.push({ kind: "status", text: `Updated ${event.artifact.label}` });
      break;
    case "ui_scene":
      projected.push({ kind: "status", text: "Updated the view" });
      break;
    case "budget_approval_required":
      projected.push({
        kind: "budget_request",
        requestId: event.request_id,
        rounds: event.requested_rounds,
      });
      pushStatus(projected, event.progress_summary);
      break;
    case "browser_challenge":
      pushStatus(projected, event.reason);
      break;
    case "browser_navigated":
      pushStatus(projected, event.title);
      break;
    case "context_usage":
      pushStatus(projected, event.operator_summary);
      break;
    case "permission_request":
      projected.push({
        kind: "permission_request",
        requestId: event.request_id,
        message: event.message,
      });
      break;
    default:
      assertNever(event);
  }

  return projected;
}

function projectTerminalText(
  projected: TurnStreamProjectedEvent[],
  state: TurnStreamProjectionState,
  rawText: string,
): void {
  const text = rawText.trim();
  if (!text) return;
  if (state.toolRuns.size > 0 || (state.answerText && !text.startsWith(state.answerText))) {
    state.answerText = text;
    projected.push({ kind: "answer_replace", text });
  } else if (state.answerText.length === 0) {
    state.answerText = text;
    projected.push({ kind: "answer_delta", text });
  } else if (text.length > state.answerText.length) {
    const suffix = text.slice(state.answerText.length);
    state.answerText = text;
    projected.push({ kind: "answer_delta", text: suffix });
  }
}

function pushStatus(projected: TurnStreamProjectedEvent[], text: string | null | undefined): void {
  const trimmed = text?.trim() ?? "";
  if (trimmed && !isTelemetry(trimmed)) projected.push({ kind: "status", text: trimmed });
}

function isTelemetry(text: string): boolean {
  const normalized = text.trim().toLowerCase();
  if (!normalized) return true;
  return [
    "interactive turn accepted",
    "agent runtime started",
    "running cognition_",
    "cognition_turn_",
    "context ",
    "orchestrator=",
    "fallback=",
    "tool=",
    "[stub",
    "[acp",
  ].some((marker) => normalized.startsWith(marker));
}

function formatToolName(name: string): string {
  return name
    .replace(/^cognition_/, "")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (character) => character.toUpperCase());
}

function friendlyError(message: string): string {
  const trimmed = message.trim();
  if (!trimmed || isTelemetry(trimmed)) {
    return "Something went wrong on this turn. Try again in a moment.";
  }
  return trimmed;
}

function assertNever(event: never): never {
  throw new Error(`Unhandled turn stream event: ${JSON.stringify(event satisfies never)}`);
}
