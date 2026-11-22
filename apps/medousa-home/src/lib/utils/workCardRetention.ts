import type { WorkCard } from "$lib/types/workspace";
import { workHubLayer } from "$lib/utils/workHub";

export function shouldHideTerminalWorkCard(
  card: WorkCard,
  hideAfterHours: number,
): boolean {
  const layer = workHubLayer(card);
  if (layer !== "failed" && layer !== "stopped" && layer !== "settled") {
    return false;
  }
  const updated = Date.parse(card.updated_at_utc);
  if (!Number.isFinite(updated)) return false;
  const ageMs = Date.now() - updated;
  return ageMs > hideAfterHours * 60 * 60 * 1000;
}

export function visibleWorkCards(
  cards: WorkCard[],
  hideAfterHours: number,
): WorkCard[] {
  return cards.filter((card) => !shouldHideTerminalWorkCard(card, hideAfterHours));
}
