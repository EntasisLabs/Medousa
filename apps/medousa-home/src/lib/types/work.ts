import type { WorkBoardColumn, WorkCard } from "$lib/types/workspace";
import type { WorkCardDetail } from "$lib/types/card";

export type SwimlaneMode = "none" | "intent" | "manuscript" | "job_family" | "session";

export type WorkView = "kanban" | "inspector";

export interface WorkspaceCardActionResponse {
  workspace_revision: number;
  card_id: string;
  action: string;
  ok: boolean;
  message: string;
  job_id?: string | null;
  replayed?: boolean | null;
  job_succeeded?: boolean | null;
}

export const KANBAN_COLUMNS: WorkBoardColumn[] = [
  "backlog",
  "in_flight",
  "wrapping_up",
  "blocked",
  "done",
];

export interface KanbanLane {
  key: string;
  label: string;
  cards: WorkCard[];
}

export interface KanbanColumn {
  column: WorkBoardColumn;
  lanes: KanbanLane[];
  cards: WorkCard[];
}

export function swimlaneLabel(
  detail: WorkCardDetail | undefined,
  mode: SwimlaneMode,
  card: WorkCard,
): string {
  if (mode === "none") return "";
  if (!detail) return "…";

  switch (mode) {
    case "intent":
      if (detail.kind === "turn_worker") {
        return detail.subtitle?.trim() || "turn";
      }
      return detail.subtitle?.trim() || detail.job_type?.trim() || "job";
    case "manuscript":
      return detail.manuscript_id?.trim() || "no manuscript";
    case "job_family":
      if (detail.job_type?.includes(".")) {
        return detail.job_type.split(".")[0];
      }
      return detail.job_type?.trim() || detail.kind;
    case "session":
      return detail.session_id?.trim() || "no session";
    default:
      return card.id;
  }
}
