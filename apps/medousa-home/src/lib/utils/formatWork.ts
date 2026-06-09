import type { WorkCard } from "$lib/types/workspace";
import { columnLabel } from "$lib/types/workspace";

/** Operator-facing card labels — not raw job status strings. */
export function formatCardTitle(card: WorkCard): string {
  const title = card.title.trim();
  if (title && !isGenericWorkflowTitle(title)) {
    return humanizeTitle(title);
  }

  if (card.status_label === "dead_letter") {
    return "Stuck — needs a look";
  }
  if (card.status_label === "failed") {
    return "Didn't finish";
  }
  if (card.status_label === "canceled") {
    return "Stopped";
  }

  switch (card.column) {
    case "in_flight":
      return "Working on your request";
    case "wrapping_up":
      return "Pulling it together";
    case "blocked":
      return "Waiting on you";
    case "backlog":
      return "Queued up";
    default:
      return title || "Work item";
  }
}

/** Second line — column + status in plain language. */
export function formatCardSubtitle(card: WorkCard): string {
  const status = formatStatusLabel(card.status_label);

  switch (card.column) {
    case "blocked":
      return `Needs you · ${status}`;
    case "wrapping_up":
      return `Almost done · ${status}`;
    case "in_flight":
      return `In progress · ${status}`;
    case "backlog":
      return `Queued · ${status}`;
    case "done":
      return status;
    default:
      return `${columnLabel(card.column)} · ${status}`;
  }
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
  return (
    /^workflow:\s*\w+/i.test(title) ||
    title.toLowerCase() === "workflow: cognitio" ||
    /^turn[_\s-]?worker$/i.test(title)
  );
}

function humanizeTitle(title: string): string {
  if (/^user_ack:/i.test(title)) {
    return title.replace(/^user_ack:\s*/i, "").trim() || title;
  }
  return title;
}
