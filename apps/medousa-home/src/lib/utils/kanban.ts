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

export function columnTone(column: WorkBoardColumn | string): string {
  switch (column) {
    case "in_flight":
      return "border-primary-500/40 bg-primary-500/5";
    case "wrapping_up":
      return "border-warning-500/50 bg-warning-500/10";
    case "blocked":
      return "border-error-500/40 bg-error-500/10";
    case "done":
      return "border-surface-500/30 bg-surface-800/40";
    default:
      return "border-surface-500/20 bg-surface-900/40";
  }
}
