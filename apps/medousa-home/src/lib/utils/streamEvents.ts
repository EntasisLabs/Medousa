import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

export function isWorkerHandoffStreamEvent(
  event: InteractiveTurnStreamEvent,
): boolean {
  return event.event_type === "worker_ack" || event.phase === "worker_ack";
}

export function isBudgetApprovalStreamEvent(
  event: InteractiveTurnStreamEvent,
): boolean {
  return (
    event.event_type === "budget_approval" || event.phase === "budget_blocked"
  );
}

export function isBrowserChallengeStreamEvent(
  event: InteractiveTurnStreamEvent,
): boolean {
  return event.event_type === "browser_challenge";
}

export function isBrowserNavigatedStreamEvent(
  event: InteractiveTurnStreamEvent,
): boolean {
  return event.event_type === "browser_navigated";
}

/** Transport-level SSE failures that should reattach rather than settle turns. */
export function isRecoverableStreamError(message: string): boolean {
  const trimmed = message.trim();
  if (!trimmed) return true;
  return (
    trimmed.includes("SSE stream ended unexpectedly") ||
    trimmed.includes("cannot reach") ||
    trimmed.startsWith("HTTP ")
  );
}

export function isHandoffStreamEvent(event: InteractiveTurnStreamEvent): boolean {
  return isWorkerHandoffStreamEvent(event) || isBudgetApprovalStreamEvent(event);
}

/** Whether a stream event should commit visible assistant body as terminal prose. */
export function isTerminalContentCommit(
  event: InteractiveTurnStreamEvent,
): boolean {
  if (isHandoffStreamEvent(event)) return false;
  return (
    event.terminal ||
    event.event_type === "final" ||
    event.event_type === "turn_checkpoint" ||
    event.event_type === "needs_input"
  );
}

export function isCheckpointHandoffStreamEvent(
  event: InteractiveTurnStreamEvent,
): boolean {
  return event.event_type === "turn_checkpoint" || event.phase === "handoff";
}
