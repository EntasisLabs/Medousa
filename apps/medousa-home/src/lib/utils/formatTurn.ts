/** Operator-facing labels for daemon turn stream phases. */

export function formatTurnPhase(phase: string): string {
  switch (phase) {
    case "tool_loop":
      return "Running tools";
    case "synthesis":
    case "continuation":
      return "Finishing answer";
    case "worker":
      return "Background worker";
    case "host":
      return "Planning";
    case "gatekeeper":
      return "Checking answer";
    case "startup":
      return "Starting";
    case "orchestration":
      return "Planning";
    case "streaming":
      return "Thinking…";
    case "handoff":
      return "Ready for you";
    case "worker_ack":
      return "Worker started";
    case "awaiting_operator":
    case "budget_blocked":
    case "budget_approval":
      return "Needs approval";
    case "final_pending":
      return "Wrapping up";
    case "tool_loop":
      return "Running tools";
    case "complete":
      return "Done";
    default:
      return phase.replaceAll("_", " ");
  }
}

export function formatToolName(tool: string): string {
  return tool
    .replace(/^cognition_/, "")
    .replaceAll("_", " ");
}
