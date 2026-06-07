import type { WorkCard } from "$lib/types/workspace";

/** Operator-facing card labels — not raw job status strings. */
export function formatCardTitle(card: WorkCard): string {
  const title = card.title.trim();
  if (title && !isGenericWorkflowTitle(title)) {
    return title;
  }

  if (card.status_label === "dead_letter") {
    return "Stuck job needs review";
  }
  if (card.status_label === "failed") {
    return "Job failed";
  }
  if (card.status_label === "canceled") {
    return "Job canceled";
  }

  return title || "Work item";
}

export function formatStatusLabel(status: string): string {
  switch (status) {
    case "dead_letter":
      return "Stuck";
    case "running":
      return "Running";
    case "leased":
      return "Starting";
    case "queued":
      return "Queued";
    case "synthesis pending":
      return "Finishing up";
    default:
      return status.replaceAll("_", " ");
  }
}

function isGenericWorkflowTitle(title: string): boolean {
  return /^workflow:\s*\w+/i.test(title) || title.toLowerCase() === "workflow: cognitio";
}
