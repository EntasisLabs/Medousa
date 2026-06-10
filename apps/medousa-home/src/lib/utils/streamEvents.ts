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
    event.event_type === "needs_input" ||
    event.event_type === "error"
  );
}
