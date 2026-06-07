import type { WorkCardDetail } from "$lib/types/card";
import type { KanbanColumn, KanbanLane, SwimlaneMode } from "$lib/types/work";
import {
  KANBAN_COLUMNS,
  swimlaneLabel,
} from "$lib/types/work";
import type { WorkBoardColumn, WorkCard } from "$lib/types/workspace";

export function filterKanbanCards(
  cards: WorkCard[],
  showDone: boolean,
): WorkCard[] {
  return cards.filter((card) => showDone || card.column !== "done");
}

export function buildKanbanColumns(
  cards: WorkCard[],
  details: Map<string, WorkCardDetail>,
  swimlane: SwimlaneMode,
  showDone: boolean,
): KanbanColumn[] {
  const visible = filterKanbanCards(cards, showDone);
  const columns = showDone
    ? KANBAN_COLUMNS
    : KANBAN_COLUMNS.filter((column) => column !== "done");

  return columns.map((column) => {
    const columnCards = visible.filter((card) => card.column === column);
    if (swimlane === "none") {
      return {
        column,
        lanes: [],
        cards: columnCards,
      };
    }

    const laneMap = new Map<string, WorkCard[]>();
    for (const card of columnCards) {
      const key = swimlaneLabel(details.get(card.id), swimlane, card);
      const bucket = laneMap.get(key) ?? [];
      bucket.push(card);
      laneMap.set(key, bucket);
    }

    const lanes: KanbanLane[] = [...laneMap.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, laneCards]) => ({
        key,
        label: key,
        cards: laneCards,
      }));

    return { column, lanes, cards: columnCards };
  });
}

/** Unified column chrome — accent lives in header dot only. */
export function columnTone(_column: WorkBoardColumn | string): string {
  return "border-surface-500/45 bg-surface-800/70 shadow-sm";
}

export function columnAccent(column: WorkBoardColumn | string): string {
  switch (column) {
    case "in_flight":
      return "bg-primary-400";
    case "wrapping_up":
      return "bg-warning-400";
    case "blocked":
      return "bg-error-400";
    case "done":
      return "bg-success-400";
    default:
      return "bg-surface-400";
  }
}

export function columnAccentBorder(column: WorkBoardColumn | string): string {
  switch (column) {
    case "in_flight":
      return "border-l-primary-500/70";
    case "wrapping_up":
      return "border-l-warning-500/70";
    case "blocked":
      return "border-l-error-500/60";
    case "done":
      return "border-l-success-500/60";
    default:
      return "border-l-surface-500/50";
  }
}
