import type { WorkspaceEvent } from "$lib/types/workspace";

const EVENT_LABELS: Record<string, string> = {
  job_enqueued: "Queued",
  job_started: "Started",
  job_succeeded: "Succeeded",
  job_failed: "Failed",
  work_delegated: "Delegated",
  work_completed: "Completed",
  work_wrapping_up: "Wrapping up",
  work_unblocked: "Unblocked",
  turn_accepted: "Turn accepted",
  identity_remembered: "Identity",
  agent_replied: "Reply",
  vault_note_created: "Vault",
  vault_note_updated: "Vault",
};

export function formatWorkspaceEventKind(kind: string): string {
  return EVENT_LABELS[kind] ?? kind.replaceAll("_", " ");
}

export function filterCardTimeline(
  events: WorkspaceEvent[],
  cardId: string,
): WorkspaceEvent[] {
  return events.filter((event) =>
    event.refs.some(
      (ref) => ref.ref_type === "card" && ref.ref_id === cardId,
    ),
  );
}

export function sortTimeline(events: WorkspaceEvent[]): WorkspaceEvent[] {
  return [...events].sort(
    (a, b) => Date.parse(b.timestamp_utc) - Date.parse(a.timestamp_utc),
  );
}

export function timelineVaultPath(event: WorkspaceEvent): string | null {
  const pathRef = event.refs.find((ref) => ref.ref_type === "vault_path");
  return pathRef?.ref_id?.trim() || null;
}

export function timelineToolNames(event: WorkspaceEvent): string[] {
  return event.tool_names?.length ? [...event.tool_names] : [];
}
