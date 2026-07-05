import { isAskJobId } from "$lib/types/askJob";
import type { WorkCard } from "$lib/types/workspace";

/** Terminal worker cards that need a detail fetch before chat synthesis can land. */
export function terminalWorkerCardsNeedingRecovery(
  cards: WorkCard[],
  previousColumns: Map<string, string>,
  pendingSynthesis: Set<string>,
): WorkCard[] {
  return cards.filter((card) => {
    if (isAskJobId(card.id)) return false;

    const terminal = card.column === "done" || card.column === "blocked";
    if (!terminal) return false;

    if (pendingSynthesis.has(card.id)) return true;

    const prev = previousColumns.get(card.id);
    return prev !== undefined && prev !== card.column;
  });
}

export const WORKER_RECOVERY_FETCH_CONCURRENCY = 4;
