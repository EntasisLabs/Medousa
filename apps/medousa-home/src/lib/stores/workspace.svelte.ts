import {
  cancelWorkspaceCard,
  getWorkspaceCard,
  retryWorkspaceCard,
} from "$lib/daemon";
import { notifyCardDone } from "$lib/notifications";
import type { WorkCardDetail } from "$lib/types/card";
import type { SwimlaneMode, WorkView } from "$lib/types/work";
import type {
  WorkCard,
  WorkspaceEvent,
  WorkspaceStreamEvent,
} from "$lib/types/workspace";

export class WorkspaceStore {
  revision = $state(0);
  cards = $state<WorkCard[]>([]);
  feed = $state<WorkspaceEvent[]>([]);
  columnCounts = $state<Record<string, number>>({});
  streamError = $state<string | null>(null);
  selectedCardId = $state<string | null>(null);
  selectedCardDetail = $state<WorkCardDetail | null>(null);
  cardDetailError = $state<string | null>(null);
  cardActionMessage = $state<string | null>(null);
  cardDetailsCache = $state<Map<string, WorkCardDetail>>(new Map());
  swimlane = $state<SwimlaneMode>("none");
  showDone = $state(false);
  workView = $state<WorkView>("kanban");
  private previousColumns = new Map<string, string>();

  applyEvent(event: WorkspaceStreamEvent) {
    this.revision = event.workspace_revision;
    this.streamError = null;

    switch (event.stream_event_type) {
      case "snapshot":
        if (event.snapshot) {
          this.cards = event.snapshot.cards;
          this.feed = event.snapshot.feed_tail;
          this.columnCounts = event.snapshot.counts_by_column;
          this.syncColumnMemory();
        }
        break;
      case "card_upserted":
        if (event.card) {
          this.handleCardUpserted(event.card);
        }
        break;
      case "card_removed":
        if (event.card) {
          const id = event.card.id;
          this.cards = this.cards.filter((c) => c.id !== id);
          this.previousColumns.delete(id);
          this.cardDetailsCache.delete(id);
          if (this.selectedCardId === id) {
            this.clearSelection();
          }
        }
        break;
      case "feed_appended":
        if (event.feed_event) {
          this.feed = [...this.feed, event.feed_event].slice(-200);
        }
        break;
      case "column_counts":
        if (event.counts) {
          this.columnCounts = event.counts;
        }
        break;
      default:
        break;
    }
  }

  private syncColumnMemory() {
    this.previousColumns.clear();
    for (const card of this.cards) {
      this.previousColumns.set(card.id, card.column);
    }
  }

  private handleCardUpserted(card: WorkCard) {
    const previous = this.previousColumns.get(card.id);
    if (previous !== "done" && card.column === "done") {
      void notifyCardDone(card.title, card.status_label);
    }
    this.previousColumns.set(card.id, card.column);

    const idx = this.cards.findIndex((c) => c.id === card.id);
    if (idx >= 0) {
      this.cards[idx] = card;
    } else {
      this.cards = [...this.cards, card];
    }

    if (this.selectedCardId === card.id) {
      void this.refreshSelectedCard();
    }

    void this.cacheCardDetail(card.id);
  }

  setError(message: string) {
    this.streamError = message;
  }

  async prefetchCardDetails() {
    const targets = this.kanbanCards().map((card) => card.id);
    await Promise.all(targets.map((id) => this.cacheCardDetail(id)));
  }

  private async cacheCardDetail(id: string) {
    try {
      const detail = await getWorkspaceCard(id);
      this.cardDetailsCache.set(id, detail);
      this.cardDetailsCache = new Map(this.cardDetailsCache);
      if (this.selectedCardId === id) {
        this.selectedCardDetail = detail;
      }
    } catch {
      // Swimlane label falls back when detail is missing.
    }
  }

  kanbanCards(): WorkCard[] {
    return this.cards.filter((card) => this.showDone || card.column !== "done");
  }

  activeCards(): WorkCard[] {
    return this.cards.filter((c) => c.column !== "done");
  }

  /** Bottom work rail — in-motion cards only (Codex-style). */
  railCards(): WorkCard[] {
    const activeColumns = new Set(["backlog", "in_flight", "wrapping_up"]);
    return this.cards.filter((card) => activeColumns.has(card.column));
  }

  async selectCard(id: string | null, options?: { inspector?: boolean }) {
    this.selectedCardId = id;
    this.selectedCardDetail = null;
    this.cardDetailError = null;
    this.cardActionMessage = null;
    if (!id) {
      this.workView = "kanban";
      return;
    }

    if (options?.inspector !== false) {
      this.workView = "inspector";
    }

    const cached = this.cardDetailsCache.get(id);
    if (cached) {
      this.selectedCardDetail = cached;
      return;
    }

    try {
      const detail = await getWorkspaceCard(id);
      this.selectedCardDetail = detail;
      this.cardDetailsCache.set(id, detail);
      this.cardDetailsCache = new Map(this.cardDetailsCache);
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }

  async refreshSelectedCard() {
    if (!this.selectedCardId) return;
    await this.selectCard(this.selectedCardId, { inspector: true });
  }

  clearSelection() {
    this.selectedCardId = null;
    this.selectedCardDetail = null;
    this.cardDetailError = null;
    this.cardActionMessage = null;
    this.workView = "kanban";
  }

  showKanban() {
    this.workView = "kanban";
  }

  async cancelSelectedCard() {
    if (!this.selectedCardId) return;
    this.cardActionMessage = null;
    try {
      const response = await cancelWorkspaceCard(this.selectedCardId);
      this.cardActionMessage = response.message;
      if (!response.ok) {
        this.cardDetailError = response.message;
      }
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }

  async retrySelectedCard() {
    if (!this.selectedCardId) return;
    this.cardActionMessage = null;
    try {
      const response = await retryWorkspaceCard(this.selectedCardId);
      this.cardActionMessage = response.message;
      if (!response.ok) {
        this.cardDetailError = response.message;
      }
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }
}

export const workspace = new WorkspaceStore();
