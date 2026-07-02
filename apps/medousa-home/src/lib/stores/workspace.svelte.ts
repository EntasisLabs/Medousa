import {
  archiveAskJob,
  archiveWorkspaceCard,
  cancelWorkspaceCard,
  enqueueDaemonAsk,
  fetchWorkspaceSnapshot,
  getWorkspaceCard,
  retryWorkspaceCard,
} from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { settings } from "$lib/stores/settings.svelte";
import type { BlockedGroup } from "$lib/utils/groupWork";
import {
  budgetWorkCardId,
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
import { friendlyUserError } from "$lib/utils/normieErrors";
import { hubCardsForPrefetch } from "$lib/utils/workHub";
import {
  isActionableBlockedCard,
  visibleWorkCards,
} from "$lib/utils/workCardRetention";
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

/** Activity kinds that should pull fresh card columns from the daemon. */
const FEED_CARD_RECONCILE_KINDS = new Set([
  "job_succeeded",
  "work_completed",
  "work_unblocked",
  "job_failed",
  "work_wrapping_up",
  "job_started",
]);

const LIVING_BOARD_COLUMNS = new Set(["backlog", "in_flight", "wrapping_up"]);

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
  private reconcilingCards = false;
  private heartbeatsSinceReconcile = 0;

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
          void this.reconcileCardsForFeedEvent(event.feed_event);
        }
        break;
      case "column_counts":
        if (event.counts) {
          this.columnCounts = event.counts;
          if (this.boardCountsDriftFromServer(event.counts)) {
            void this.reconcileCardsFromSnapshot();
          }
        }
        break;
      case "heartbeat":
        this.heartbeatsSinceReconcile += 1;
        if (this.heartbeatsSinceReconcile >= 6) {
          this.heartbeatsSinceReconcile = 0;
          void this.reconcileCardsFromSnapshot();
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
    const transitionedToDone =
      previous !== undefined && previous !== "done" && card.column === "done";

    if (transitionedToDone) {
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
      card.status_label === "needs approval" &&
      !chat.hasPendingBudgetApproval(card.id)
    ) {
      void notifyBudgetApprovalRequired(
        card.title,
        budgetWorkCardId(card.id),
        card.status_label,
      );
    }
    if (
      previous === "in_flight" &&
      card.column === "done" &&
      !isAskJobId(card.id)
    ) {
      chat.noteBackgroundSettled();
      void this.syncTurnWorkerCardsToChat();
    }
    if (
      previous === "wrapping_up" &&
      card.column === "done" &&
      !isAskJobId(card.id)
    ) {
      void this.syncTurnWorkerCardsToChat();
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
      this.cards = this.cards.map((existing, index) =>
        index === idx ? card : existing,
      );
    } else {
      this.cards = [...this.cards, card];
    }

    if (this.pendingFocusJobId && card.id === this.pendingFocusJobId) {
      this.pendingFocusJobId = null;
      void this.selectCard(card.id);
    } else if (this.selectedCardId === card.id) {
      void this.refreshSelectedCard();
    }

    if (this.shouldRefreshCardDetail(card, previous)) {
      void this.cacheCardDetail(card.id, previous, true);
    } else if (
      this.shouldPrefetchDetail(card) &&
      !this.cardDetailsCache.has(card.id)
    ) {
      void this.cacheCardDetail(card.id, previous);
    } else if (previous !== card.column) {
      chat.syncWorkerLaneFromCards(this.cards, this.cardDetailsCache);
    }
  }

  /** Tier 3 — scan cached turn_worker cards and deliver pending syntheses to chat. */
  async syncTurnWorkerCardsToChat() {
    chat.syncWorkerLaneFromCards(this.cards, this.cardDetailsCache);
    await chat.recoverPendingWorkerSyntheses(this.cards, this.cardDetailsCache);
  }

  /** Pull authoritative card columns when activity says done but the board lagged. */
  async reconcileCardsFromSnapshot() {
    if (this.reconcilingCards) return;
    this.reconcilingCards = true;
    try {
      const snapshot = await fetchWorkspaceSnapshot(
        this.revision > 0 ? this.revision : undefined,
      );
      if (snapshot.cards.length === 0 && this.revision > 0) {
        return;
      }
      this.revision = snapshot.workspace_revision;
      this.cards = snapshot.cards;
      this.columnCounts = snapshot.counts_by_column;
      this.syncColumnMemory();
      chat.syncWorkerLaneFromCards(this.cards, this.cardDetailsCache);
    } catch (err) {
      this.streamError = err instanceof Error ? err.message : String(err);
    } finally {
      this.reconcilingCards = false;
    }
  }

  private boardCountsDriftFromServer(counts: Record<string, number>): boolean {
    const living = ["backlog", "in_flight", "wrapping_up"] as const;
    for (const column of living) {
      const local = this.cards.filter((card) => card.column === column).length;
      const remote = counts[column] ?? 0;
      if (local !== remote) return true;
    }
    return false;
  }

  private async reconcileCardsForFeedEvent(event: WorkspaceEvent) {
    if (!FEED_CARD_RECONCILE_KINDS.has(event.kind)) return;
    const cardIds = [
      ...new Set(
        event.refs
          .filter((ref) => ref.ref_type === "card")
          .map((ref) => ref.ref_id.trim())
          .filter(Boolean),
      ),
    ];
    if (cardIds.length === 0) return;

    const staleIds = cardIds.filter((id) => {
      const local = this.cards.find((card) => card.id === id);
      return local != null && LIVING_BOARD_COLUMNS.has(local.column);
    });
    if (staleIds.length === 0) {
      return;
    }

    await Promise.all(staleIds.map((id) => this.refreshCardProjection(id)));
  }

  private async refreshCardProjection(cardId: string) {
    try {
      const detail = await getWorkspaceCard(cardId);
      this.cardDetailsCache.set(cardId, detail);
      this.handleCardUpserted(detail.card);
    } catch {
      // Card may have been archived between feed and fetch.
    }
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
    this.streamError = friendlyUserError(message);
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

  private activityPrefetchTimer: ReturnType<typeof setTimeout> | null = null;

  scheduleActivityCardPrefetch() {
    if (this.activityPrefetchTimer) {
      clearTimeout(this.activityPrefetchTimer);
    }
    this.activityPrefetchTimer = setTimeout(() => {
      this.activityPrefetchTimer = null;
      void this.prefetchActivityCardDetails();
    }, 300);
  }

  async prefetchActivityCardDetails() {
    const targets = this.activityCardIds().filter(
      (id) => !this.cardDetailsCache.has(id),
    );
    const concurrency = 3;
    for (let i = 0; i < targets.length; i += concurrency) {
      const batch = targets.slice(i, i + concurrency);
      await Promise.all(batch.map((id) => this.cacheCardDetail(id)));
    }
  }

  private shouldPrefetchDetail(card: WorkCard): boolean {
    return (
      LIVING_DETAIL_COLUMNS.has(card.column) || card.id === this.selectedCardId
    );
  }

  /** Re-fetch card detail when a known worker/ask card reaches a terminal column. */
  private shouldRefreshCardDetail(card: WorkCard, previous?: string): boolean {
    if (previous === card.column) return false;
    if (card.column !== "done" && card.column !== "blocked") return false;
    const cached = this.cardDetailsCache.get(card.id);
    if (cached?.kind === "turn_worker") return true;
    return isAskJobId(card.id);
  }

  private async cacheCardDetail(
    id: string,
    previousColumn?: string,
    force = false,
  ) {
    if (!force) {
      const cached = this.cardDetailsCache.get(id);
      if (cached) {
        const card = this.cards.find((item) => item.id === id);
        if (card) {
          chat.onWorkerCardDetail(cached, card.column, previousColumn);
          chat.syncWorkerLaneFromCards(this.cards, this.cardDetailsCache);
          if (
            cached.kind === "turn_worker" &&
            (card.column === "done" ||
              (card.column === "blocked" && cached.terminal))
          ) {
            void chat.recoverPendingWorkerSyntheses([card], this.cardDetailsCache);
          }
        }
        return;
      }
    }
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
        chat.syncWorkerLaneFromCards(this.cards, this.cardDetailsCache);
        if (
          detail.kind === "turn_worker" &&
          (card.column === "done" ||
            (card.column === "blocked" && detail.terminal))
        ) {
          void chat.recoverPendingWorkerSyntheses([card], this.cardDetailsCache);
        }
      }
    } catch {
      // Swimlane label falls back when detail is missing.
    }
  }

  kanbanCards(): WorkCard[] {
    const cards = this.visibleCards();
    return cards.filter((card) => this.showDone || card.column !== "done");
  }

  activeCards(): WorkCard[] {
    return this.visibleCards().filter((c) => c.column !== "done");
  }

  visibleCards(): WorkCard[] {
    return visibleWorkCards(this.cards, settings.workCardHideAfterHours);
  }

  /** Bottom work rail — in-motion cards only (Codex-style). */
  railCards(): WorkCard[] {
    const activeColumns = new Set(["backlog", "in_flight", "wrapping_up"]);
    return this.cards.filter((card) => activeColumns.has(card.column));
  }

  blockedCount(): number {
    return this.visibleCards().filter(isActionableBlockedCard).length;
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

  inMotionCardsForVaultPath(path: string): WorkCard[] {
    return this.railCards().filter((card) => {
      const detail = this.cardDetailsCache.get(card.id);
      return detail?.associations.vault_paths.includes(path) ?? false;
    });
  }

  async prefetchVaultLinkedWork(_notePath: string) {
    const targets = this.railCards()
      .filter((card) => !this.cardDetailsCache.has(card.id))
      .map((card) => card.id);
    await Promise.all(targets.map((id) => this.cacheCardDetail(id)));
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
    if (detail?.work_id || detail?.kind === "turn_worker") {
      const response = await archiveWorkspaceCard(id, purgeOutput);
      return { ok: response.ok, message: response.message };
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

  async archiveTerminalTrayCards(cards: WorkCard[], label: string, limit = 24) {
    this.cardActionMessage = null;
    this.cardDetailError = null;
    let ok = 0;
    let failed = 0;

    for (const card of cards.slice(0, limit)) {
      try {
        const response = await this.archiveCard(card.id, true);
        if (response.ok) ok += 1;
        else failed += 1;
      } catch {
        failed += 1;
      }
    }

    this.cardActionMessage = `Cleared ${ok} ${label}${failed ? ` · ${failed} failed` : ""}`;
  }
}

export const workspace = new WorkspaceStore();
