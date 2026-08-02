import type { InteractiveTurnStreamEvent } from "@medousa/client";

export type ProjectedEvent =
  | { kind: "answer_delta"; text: string }
  | { kind: "answer_replace"; text: string }
  | { kind: "status"; text: string }
  | { kind: "tool_started"; name: string }
  | { kind: "tool_finished"; name: string; status: string }
  | { kind: "budget_request"; requestId: string; rounds: number }
  | { kind: "permission_request"; requestId: string; message: string }
  | { kind: "terminal"; text?: string; error?: boolean };

export interface ProjectionState {
  answerText: string;
  toolRuns: Set<string>;
}

export function createProjectionState(): ProjectionState {
  return { answerText: "", toolRuns: new Set() };
}

export function projectStreamEvent(
  event: InteractiveTurnStreamEvent,
  state: ProjectionState,
): ProjectedEvent[] {
  const projected: ProjectedEvent[] = [];
  const eventType = event.event_type.toLowerCase();

  if (event.permission_request_id) {
    projected.push({
      kind: "permission_request",
      requestId: event.permission_request_id,
      message: event.operator_message ?? "Medousa needs permission to continue.",
    });
  }
  if (event.budget_request_id) {
    projected.push({
      kind: "budget_request",
      requestId: event.budget_request_id,
      rounds: event.requested_rounds ?? 1,
    });
  }

  if (eventType === "tool_started" || (event.tool_name && event.tool_status === "running")) {
    const runId = event.tool_run_id ?? `${event.tool_name ?? "tool"}-${event.tool_round ?? 1}`;
    state.toolRuns.add(runId);
    projected.push({ kind: "tool_started", name: formatToolName(event.tool_name ?? "tool") });
  }

  if (eventType === "tool_finished" || (event.tool_name && event.tool_status && event.tool_status !== "running")) {
    const runId = event.tool_run_id ?? `${event.tool_name ?? "tool"}-${event.tool_round ?? 1}`;
    state.toolRuns.add(runId);
    projected.push({
      kind: "tool_finished",
      name: formatToolName(event.tool_name ?? "tool"),
      status: event.tool_status ?? (eventType === "tool_finished" ? "succeeded" : "finished"),
    });
  }

  if (event.content_delta && state.toolRuns.size === 0) {
    state.answerText += event.content_delta;
    projected.push({ kind: "answer_delta", text: event.content_delta });
  }

  if (event.operator_message && !isTelemetry(event.operator_message)) {
    projected.push({ kind: "status", text: event.operator_message });
  }

  if (event.terminal) {
    const finalText = event.final_text?.trim() ?? "";
    if (finalText && (state.toolRuns.size > 0 || state.answerText !== finalText)) {
      state.answerText = finalText;
      projected.push({ kind: "answer_replace", text: finalText });
    } else if (finalText && state.answerText.length === 0) {
      state.answerText = finalText;
      projected.push({ kind: "answer_delta", text: finalText });
    }
    const failed = eventType === "error" || event.phase.toLowerCase().includes("error") || event.tool_status === "failed";
    projected.push({
      kind: "terminal",
      text: failed ? friendlyError(event.operator_message ?? event.message) : undefined,
      error: failed,
    });
    return projected;
  }

  return projected;
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
  if (!trimmed || isTelemetry(trimmed)) return "Something went wrong on this turn. Try again in a moment.";
  return trimmed;
}
