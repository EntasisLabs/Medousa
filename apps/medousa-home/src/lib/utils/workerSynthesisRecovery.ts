import { isAskJobId } from "$lib/types/askJob";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";

export function awaitingWorkerResultDelivery(detail: WorkCardDetail): boolean {
  return detail.wrapping_up_reasons.some(
    reason => reason === "delivery_pending" || reason === "delivery_delayed",
  );
}

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
