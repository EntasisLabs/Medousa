import {
  archiveAskJob,
  cancelWorkspaceCard,
  enqueueDaemonAsk,
  getWorkspaceCard,
  retryWorkspaceCard,
} from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import type { BlockedGroup } from "$lib/utils/groupWork";
import {
  notifyAskComplete,
  notifyBudgetApprovalRequired,
  notifyCardDone,
} from "$lib/notifications";
import { isTauriMobilePlatform } from "$lib/platform";
import type { PendingAskCompletion } from "$lib/types/askJob";
import { isAskJobId } from "$lib/types/askJob";
import type { WorkCardDetail } from "$lib/types/card";
import type { SwimlaneMode, WorkView } from "$lib/types/work";
import type { EnqueueAskJobRequest } from "$lib/utils/askPrompt";
import { collectActivityCardIds } from "$lib/utils/activityEnrichment";
import { hubCardsForPrefetch } from "$lib/utils/workHub";
import type {
  WorkCard,
  WorkspaceEvent,
  WorkspaceStreamEvent,
} from "$lib/types/workspace";

const LIVING_DETAIL_COLUMNS = new Set([
  "backlog",
  "in_flight",
  "wrapping_up",
  "blocked",
]);

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
  askSubmitting = $state(false);
  askError = $state<string | null>(null);
  askMessage = $state<string | null>(null);
  pendingFocusJobId = $state<string | null>(null);
  pendingAskCompletion = $state<PendingAskCompletion | null>(null);
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
      if (isAskJobId(card.id)) {
        void notifyAskComplete(card.title, card.id);
        this.pendingAskCompletion = {
          jobId: card.id,
          title: card.title,
        };
        chat.noteAskTurnSettled(card.id);
      } else {
        void notifyCardDone(card.title, card.status_label, card.id);
      }
    }
    if (
      isAskJobId(card.id) &&
      previous !== "blocked" &&
      card.column === "blocked" &&
      card.status_label !== "needs approval"
    ) {
      chat.noteAskTurnSettled(card.id);
    }
    if (
      isTauriMobilePlatform() &&
      previous !== "blocked" &&
      card.column === "blocked" &&
      card.status_label === "needs approval"
    ) {
      void notifyBudgetApprovalRequired(card.title, card.id);
    }
    if (
      previous === "in_flight" &&
      card.column === "done" &&
      !isAskJobId(card.id)
    ) {
      chat.noteBackgroundSettled();
    }
    if (
      previous === "blocked" &&
      card.status_label !== "needs approval"
    ) {
      chat.noteBackgroundSettled();
      chat.noteBudgetResolved(card.id);
    }
    this.previousColumns.set(card.id, card.column);

    const idx = this.cards.findIndex((c) => c.id === card.id);
    if (idx >= 0) {
      this.cards[idx] = card;
    } else {
      this.cards = [...this.cards, card];
    }

    if (this.pendingFocusJobId && card.id === this.pendingFocusJobId) {
      this.pendingFocusJobId = null;
      void this.selectCard(card.id);
    } else if (this.selectedCardId === card.id) {
      void this.refreshSelectedCard();
    }

    if (this.shouldPrefetchDetail(card)) {
      void this.cacheCardDetail(card.id, previous);
    }
  }

  /** Tier 3 — scan cached turn_worker cards and deliver pending syntheses to chat. */
  async syncTurnWorkerCardsToChat() {
    await chat.recoverPendingWorkerSyntheses(this.cards, this.cardDetailsCache);
  }

  async submitAsk(request: EnqueueAskJobRequest) {
    const trimmed = request.prompt.trim();
    const hasSkills =
      Boolean(request.manuscriptId) ||
      (request.additionalManuscriptIds?.length ?? 0) > 0;
    if ((!trimmed && !hasSkills) || this.askSubmitting) return;

    this.askSubmitting = true;
    this.askError = null;
    this.askMessage = null;

    try {
      const accepted = await enqueueDaemonAsk({
        prompt: request.prompt,
        modelHint: request.modelHint,
        manuscriptId: request.manuscriptId,
        additionalManuscriptIds: request.additionalManuscriptIds,
        suggestedCapabilityIds: request.suggestedCapabilityIds,
      });
      this.pendingFocusJobId = accepted.job_id;
      this.askMessage = `Queued · job ${accepted.job_id}`;
      await this.selectCard(accepted.job_id);
    } catch (err) {
      this.askError = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.askSubmitting = false;
    }
  }

  clearPendingAskCompletion() {
    this.pendingAskCompletion = null;
  }

  setError(message: string) {
    this.streamError = message;
  }

  async prefetchCardDetails() {
    const targets = hubCardsForPrefetch(this.cards)
      .map((card) => card.id)
      .filter((id) => !this.cardDetailsCache.has(id));
    const concurrency = 3;
    for (let i = 0; i < targets.length; i += concurrency) {
      const batch = targets.slice(i, i + concurrency);
      await Promise.all(batch.map((id) => this.cacheCardDetail(id)));
    }
  }

  activityCardIds(): string[] {
    return collectActivityCardIds(this.feed);
  }

  async prefetchActivityCardDetails() {
    const targets = this.activityCardIds();
    await Promise.all(targets.map((id) => this.cacheCardDetail(id)));
  }

  private shouldPrefetchDetail(card: WorkCard): boolean {
    return (
      LIVING_DETAIL_COLUMNS.has(card.column) || card.id === this.selectedCardId
    );
  }

  private async cacheCardDetail(id: string, previousColumn?: string) {
    try {
      const detail = await getWorkspaceCard(id);
      this.cardDetailsCache.set(id, detail);
      this.cardDetailsCache = new Map(this.cardDetailsCache);
      if (this.selectedCardId === id) {
        this.selectedCardDetail = detail;
      }
      const card = this.cards.find((item) => item.id === id);
      if (card) {
        chat.onWorkerCardDetail(detail, card.column, previousColumn);
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

  blockedCount(): number {
    if (this.columnCounts.blocked !== undefined) {
      return this.columnCounts.blocked;
    }
    return this.cards.filter((card) => card.column === "blocked").length;
  }

  inMotionCount(): number {
    return this.railCards().length;
  }

  needsAttentionCount(): number {
    return this.blockedCount();
  }

  /** First in-motion card for Home hero — in flight, then wrapping up, then backlog. */
  primaryInMotionCard(): WorkCard | null {
    for (const column of ["in_flight", "wrapping_up", "backlog"] as const) {
      const card = this.cards.find((item) => item.column === column);
      if (card) return card;
    }
    return null;
  }

  async selectCard(id: string | null) {
    this.selectedCardId = id;
    this.selectedCardDetail = null;
    this.cardDetailError = null;
    this.cardActionMessage = null;
    if (!id) {
      return;
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
    await this.selectCard(this.selectedCardId);
  }

  clearSelection() {
    this.selectedCardId = null;
    this.selectedCardDetail = null;
    this.cardDetailError = null;
    this.cardActionMessage = null;
  }

  showKanban() {
    this.clearSelection();
  }

  async cancelCard(id: string) {
    return cancelWorkspaceCard(id);
  }

  async cancelSelectedCard() {
    if (!this.selectedCardId) return;
    this.cardActionMessage = null;
    try {
      const response = await this.cancelCard(this.selectedCardId);
      this.cardActionMessage = response.message;
      if (!response.ok) {
        this.cardDetailError = response.message;
      }
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }

  isCancellable(card: WorkCard): boolean {
    return ["backlog", "in_flight", "wrapping_up"].includes(card.column);
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

  async retryBlockedGroup(group: BlockedGroup) {
    this.cardActionMessage = null;
    this.cardDetailError = null;
    let ok = 0;
    let skipped = 0;
    let failed = 0;

    for (const card of group.cards) {
      const detail = this.cardDetailsCache.get(card.id);
      if (detail?.kind !== "stasis_job") {
        skipped += 1;
        continue;
      }
      try {
        const response = await retryWorkspaceCard(card.id);
        if (response.ok) ok += 1;
        else failed += 1;
      } catch {
        failed += 1;
      }
    }

    this.cardActionMessage = `Retried ${ok} of ${group.cards.length}${
      skipped ? ` · ${skipped} skipped (not replayable jobs)` : ""
    }${failed ? ` · ${failed} failed` : ""}`;
    if (this.selectedCardId) {
      await this.refreshSelectedCard();
    }
  }

  async dismissBlockedGroup(group: BlockedGroup) {
    this.cardActionMessage = null;
    this.cardDetailError = null;
    let ok = 0;
    let skipped = 0;
    let failed = 0;

    for (const card of group.cards) {
      const detail = this.cardDetailsCache.get(card.id);
      if (!detail || detail.terminal) {
        skipped += 1;
        continue;
      }
      if (!this.isCancellable(card)) {
        skipped += 1;
        continue;
      }
      try {
        const response = await cancelWorkspaceCard(card.id);
        if (response.ok) ok += 1;
        else failed += 1;
      } catch {
        failed += 1;
      }
    }

    this.cardActionMessage = `Hidden ${ok} of ${group.cards.length}${
      skipped ? ` · ${skipped} already terminal or not cancelable` : ""
    }${failed ? ` · ${failed} failed` : ""}`;
    if (this.selectedCardId) {
      await this.refreshSelectedCard();
    }
  }

  async archiveCard(
    id: string,
    purgeOutput = true,
  ): Promise<{ ok: boolean; message: string }> {
    const detail =
      this.cardDetailsCache.get(id) ??
      (await getWorkspaceCard(id).catch(() => null));
    if (detail?.job_id && isAskJobId(detail.job_id)) {
      const response = await archiveAskJob(detail.job_id, purgeOutput);
      return { ok: response.archived, message: response.message };
    }
    const response = await cancelWorkspaceCard(id);
    return { ok: response.ok, message: response.message };
  }

  async archiveSelectedCard() {
    if (!this.selectedCardId) return;
    this.cardActionMessage = null;
    this.cardDetailError = null;
    try {
      const response = await this.archiveCard(this.selectedCardId, true);
      this.cardActionMessage = response.message;
      if (!response.ok) {
        this.cardDetailError = response.message;
      } else {
        this.clearSelection();
      }
    } catch (err) {
      this.cardDetailError = err instanceof Error ? err.message : String(err);
    }
  }

  async archiveTrayCards(cards: WorkCard[], limit = 24) {
    this.cardActionMessage = null;
    this.cardDetailError = null;
    let ok = 0;
    let skipped = 0;
    let failed = 0;

    for (const card of cards.slice(0, limit)) {
      if (!isAskJobId(card.id) && card.column !== "done") {
        skipped += 1;
        continue;
      }
      try {
        const response = await this.archiveCard(card.id, true);
        if (response.ok) ok += 1;
        else failed += 1;
      } catch {
        failed += 1;
      }
    }

    this.cardActionMessage = `Archived ${ok}${skipped ? ` · ${skipped} skipped` : ""}${
      failed ? ` · ${failed} failed` : ""
    }`;
  }
}

export const workspace = new WorkspaceStore();
