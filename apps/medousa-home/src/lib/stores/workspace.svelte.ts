import { getWorkspaceCard } from "$lib/daemon";
import type { WorkCardDetail } from "$lib/types/card";
import type {
  WorkCard,
  WorkspaceEvent,
  WorkspaceStreamEvent,
} from "$lib/types/workspace";

export class WorkspaceStore {
  revision = $state(0);
  cards = $state<WorkCard[]>([]);
  feed = $state<WorkspaceEvent[]>([]);
  streamError = $state<string | null>(null);
  selectedCardId = $state<string | null>(null);
  selectedCardDetail = $state<WorkCardDetail | null>(null);
  cardDetailError = $state<string | null>(null);

  applyEvent(event: WorkspaceStreamEvent) {
    this.revision = event.workspace_revision;
    this.streamError = null;

    switch (event.stream_event_type) {
      case "snapshot":
        if (event.snapshot) {
          this.cards = event.snapshot.cards;
          this.feed = event.snapshot.feed_tail;
        }
        break;
      case "card_upserted":
        if (event.card) {
          const id = event.card.id;
          const idx = this.cards.findIndex((c) => c.id === id);
          if (idx >= 0) {
            this.cards[idx] = event.card;
          } else {
            this.cards = [...this.cards, event.card];
          }
        }
        break;
      case "card_removed":
        if (event.card) {
          const id = event.card.id;
          this.cards = this.cards.filter((c) => c.id !== id);
          if (this.selectedCardId === id) this.selectedCardId = null;
        }
        break;
      case "feed_appended":
        if (event.feed_event) {
          this.feed = [...this.feed, event.feed_event].slice(-200);
        }
        break;
      default:
        break;
    }
  }

  setError(message: string) {
    this.streamError = message;
  }

  async selectCard(id: string | null) {
    this.selectedCardId = id;
    this.selectedCardDetail = null;
    this.cardDetailError = null;
    if (!id) return;
    try {
      this.selectedCardDetail = await getWorkspaceCard(id);
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }

  activeCards(): WorkCard[] {
    return this.cards.filter((c) => c.column !== "done");
  }
}

export const workspace = new WorkspaceStore();
