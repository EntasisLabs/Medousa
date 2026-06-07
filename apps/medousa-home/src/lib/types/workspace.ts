export type WorkBoardColumn =
  | "backlog"
  | "in_flight"
  | "wrapping_up"
  | "done"
  | "blocked";

export interface WorkCard {
  id: string;
  column: WorkBoardColumn | string;
  title: string;
  status_label: string;
  created_at_utc: string;
  updated_at_utc: string;
}

export interface WorkspaceEvent {
  id: string;
  timestamp_utc: string;
  kind: string;
  actor: string;
  summary: string;
  refs: { ref_type: string; ref_id: string }[];
}

export interface WorkspaceSnapshot {
  workspace_revision: number;
  server_time_utc: string;
  cards: WorkCard[];
  counts_by_column: Record<string, number>;
  feed_tail: WorkspaceEvent[];
}

export interface WorkspaceStreamEvent {
  workspace_revision: number;
  stream_event_type: string;
  emitted_at_utc: string;
  card?: WorkCard;
  feed_event?: WorkspaceEvent;
  counts?: Record<string, number>;
  snapshot?: WorkspaceSnapshot;
}

export function columnLabel(column: string): string {
  return column.replaceAll("_", " ");
}
