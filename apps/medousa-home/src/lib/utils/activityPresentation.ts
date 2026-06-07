import type { WorkspaceEvent } from "$lib/types/workspace";
import {
  buildActivityContext,
  resolveTaskTitle,
  vaultRefPath,
  type ActivityEnrichment,
} from "$lib/utils/activityEnrichment";
import { formatWorkspaceEventKind } from "$lib/utils/cardTimeline";

export type ActivityTone = "success" | "motion" | "attention" | "neutral" | "vault";

export interface ActivityPresentation {
  label: string;
  tone: ActivityTone;
  summary: string;
  context?: string;
  time: string;
}

export type { ActivityEnrichment };

const TONE_BY_KIND: Record<string, ActivityTone> = {
  job_succeeded: "success",
  work_completed: "success",
  work_unblocked: "success",
  vault_note_updated: "vault",
  job_failed: "attention",
  work_delegated: "motion",
  job_started: "motion",
  job_enqueued: "motion",
  work_wrapping_up: "motion",
  turn_accepted: "neutral",
};

function simplifyFragment(text: string): string {
  return text
    .replace(/^Workflow:\s*/i, "")
    .replace(/^Job:\s*/i, "")
    .replace(/^cognition_/gi, "")
    .replaceAll("_", " ")
    .replace(/\s+/g, " ")
    .trim();
}

function humanizeVaultPath(path: string): string {
  const name = path.split("/").pop() ?? path;
  return name.replace(/\.md$/i, "").replaceAll("-", " ");
}

function enrichedSummary(
  event: WorkspaceEvent,
  enrichment?: ActivityEnrichment,
): string | null {
  if (!enrichment?.card && !enrichment?.detail) return null;

  const taskTitle = resolveTaskTitle(enrichment);

  if (event.kind === "vault_note_updated") {
    const vaultPath = vaultRefPath(event);
    if (vaultPath) return `Linked ${humanizeVaultPath(vaultPath)}`;
    if (taskTitle) return taskTitle;
    return null;
  }

  switch (event.kind) {
    case "job_succeeded":
    case "work_completed":
      return taskTitle ? `Finished ${taskTitle}` : null;
    case "work_delegated":
      return taskTitle ? `Handed off — ${taskTitle}` : null;
    case "job_started":
      return taskTitle ? `Started ${taskTitle}` : null;
    case "job_enqueued":
      return taskTitle ? `Queued ${taskTitle}` : null;
    case "job_failed":
      return taskTitle ? `Failed on ${taskTitle}` : null;
    case "work_wrapping_up":
      return taskTitle ? `Wrapping up ${taskTitle}` : null;
    case "work_unblocked":
      return taskTitle ? `Ready for you — ${taskTitle}` : null;
    default:
      return taskTitle || null;
  }
}

function humanizeSummary(event: WorkspaceEvent): string {
  const raw = event.summary.trim();
  if (!raw) return formatWorkspaceEventKind(event.kind);

  const parts = raw.split(" — ").map((part) => part.trim());
  const tail = parts.length > 1 ? simplifyFragment(parts.slice(1).join(" ")) : "";

  switch (event.kind) {
    case "job_succeeded":
    case "work_completed":
      return tail ? `Finished ${tail}` : parts[0] || raw;
    case "work_delegated":
      return tail ? `Handed off to ${tail}` : parts[0] || raw;
    case "job_started":
      return tail ? `Started ${tail}` : parts[0] || raw;
    case "job_enqueued":
      return tail ? `Queued ${tail}` : parts[0] || raw;
    case "job_failed":
      return tail ? `Failed on ${tail}` : parts[0] || raw;
    case "work_wrapping_up":
      return tail ? `Wrapping up ${tail}` : "Finishing up";
    case "work_unblocked":
      return tail ? `Ready for you — ${tail}` : "Needs your input";
    case "vault_note_updated":
      return tail || parts[0] || "Vault updated";
    default:
      if (parts.length > 1 && tail) {
        return `${parts[0]} · ${tail}`;
      }
      return raw.length > 88 ? `${raw.slice(0, 87)}…` : raw;
  }
}

function formatActivityTime(iso: string): string {
  try {
    const date = new Date(iso);
    const diffMs = Date.now() - date.getTime();
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  } catch {
    return iso;
  }
}

export function presentActivityEvent(
  event: WorkspaceEvent,
  enrichment?: ActivityEnrichment,
): ActivityPresentation {
  const summary =
    enrichedSummary(event, enrichment) ?? humanizeSummary(event);
  const context = enrichment
    ? buildActivityContext(event, enrichment).trim()
    : "";

  return {
    label: formatWorkspaceEventKind(event.kind),
    tone: TONE_BY_KIND[event.kind] ?? "neutral",
    summary,
    context: context || undefined,
    time: formatActivityTime(event.timestamp_utc),
  };
}
