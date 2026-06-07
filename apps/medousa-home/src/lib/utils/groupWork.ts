import type { WorkCard } from "$lib/types/workspace";
import { formatCardTitle } from "$lib/utils/formatWork";

export const BLOCKED_COLUMN_CAP = 8;

export interface BlockedGroup {
  key: string;
  title: string;
  cards: WorkCard[];
}

export interface BlockedColumnItem {
  type: "group";
  card: WorkCard;
  count: number;
}

export function findBlockedGroupForCard(
  cards: WorkCard[],
  cardId: string,
): BlockedGroup | null {
  const card = cards.find((item) => item.id === cardId);
  if (!card || card.column !== "blocked") return null;
  const key = `${formatCardTitle(card)}::${card.status_label}`;
  const groupCards = cards.filter(
    (item) =>
      item.column === "blocked" &&
      `${formatCardTitle(item)}::${item.status_label}` === key,
  );
  if (groupCards.length === 0) return null;
  return {
    key,
    title: formatCardTitle(groupCards[0]),
    cards: groupCards,
  };
}

export function groupBlockedCards(cards: WorkCard[]): BlockedGroup[] {
  const blocked = cards.filter((card) => card.column === "blocked");
  const map = new Map<string, WorkCard[]>();

  for (const card of blocked) {
    const key = `${formatCardTitle(card)}::${card.status_label}`;
    const bucket = map.get(key) ?? [];
    bucket.push(card);
    map.set(key, bucket);
  }

  return [...map.entries()].map(([key, groupCards]) => ({
    key,
    title: formatCardTitle(groupCards[0]),
    cards: groupCards,
  }));
}

/** One row per failure type; cap total rows; remainder as overflow count. */
export function prepareBlockedColumn(
  cards: WorkCard[],
  cap = BLOCKED_COLUMN_CAP,
): { items: BlockedColumnItem[]; overflow: number; total: number } {
  const groups = groupBlockedCards(cards);
  const items: BlockedColumnItem[] = [];
  let represented = 0;

  for (const group of groups) {
    if (items.length >= cap) break;
    items.push({
      type: "group",
      card: group.cards[0],
      count: group.cards.length,
    });
    represented += group.cards.length;
  }

  const total = cards.filter((card) => card.column === "blocked").length;
  return {
    items,
    overflow: Math.max(0, total - represented),
    total,
  };
}
