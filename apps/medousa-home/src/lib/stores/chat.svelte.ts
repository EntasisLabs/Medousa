import type {
  ChatMessage,
  ContextUsageReport,
  InteractiveTurnStreamEvent,
  PendingAgentPermission,
  PendingAgentSecret,
  PendingBudgetApproval,
  PendingBrowserChallenge,
  TurnTicketState,
} from "$lib/types/chat";
import type { TurnStreamEnvelopeV2 } from "$lib/types/generated/daemon_api";
import type { WorkCardDetail } from "$lib/types/card";
import type { SessionSummary, TurnTicketRecord, TurnTicketResponse } from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import type { MediaRef } from "$lib/types/media";
import { dedupeMessagesById } from "$lib/utils/mergeTranscript";
import type { StreamOwner } from "$lib/utils/streamOwnership";
import { StreamEventPump, type StreamEventTarget } from "$lib/stream/eventPump";
import { friendlyUserError } from "$lib/utils/normieErrors";
import type { VaultNoteContextScope } from "$lib/utils/vaultNoteBridge";
import type { ScriptWorkbenchContextScope } from "$lib/utils/scriptWorkbenchBridge";
import {
  emptySessionRuntime,
  type ChatSessionRuntime,
  type WorkerLink,
} from "$lib/chat/chatSessionRuntime";
import { chatStreamPool } from "$lib/chat/chatStreamPool.svelte";
import { MAX_SHELL_PANES } from "$lib/types/shellTabs";
import {
  clearDraftForSession,
  DRAFT_PERSIST_DEBOUNCE_MS,
  loadDraftForSession,
  persistDraftForSession,
} from "$lib/chat/draftPersistence";
import {
  selectDraftFor,
  selectMessagesFor,
  selectStreamErrorFor,
} from "$lib/chat/chatSelectors";
import type { ChatMessageIndexes, ChatStoreHost } from "$lib/chat/chatStoreHost";
import {
  currentSessionLabel,
  deleteSession as deleteSessionCtrl,
  ensureSessionHydrated,
  forkSessionFromEntry,
  hydrateAskThreads,
  isPinned,
  loadPinnedIds,
  loadPromotedAskIds,
  loadSessionId,
  newSession as newSessionCtrl,
  newSharedRoom,
  promoteAskToChat,
  reconcileOnResume,
  refreshSessions,
  reloadCurrentSession,
  renameSession,
  scheduleSessionsRefresh,
  switchSession,
  togglePin,
  warmBackgroundSession,
} from "$lib/chat/sessionController";
import {
  applyPumpedStreamEvent,
  applyStreamEventToMessage as applyStreamEventToMessageCtrl,
  attachOrphanStream,
  syncTurnFromEvent as syncTurnFromEventCtrl,
} from "$lib/chat/streamApplyController";
import {
  beginTurn as beginTurnCtrl,
  cancelActiveTurn,
  clearActiveTurn,
  clearOrphanedInteractiveTurns,
  detachStreamOwner,
  detachStreamsForSession,
  evictStreamOwners,
  finishMessage as finishMessageCtrl,
  finishStream as finishStreamCtrl,
  isComposerOpenDuringHandoff,
  isDetachedWorkerTurnRecord,
  markMessageFailed as markMessageFailedCtrl,
  noteAskTurnSettled,
  noteBackgroundSettled,
  noteStreamFailure as noteStreamFailureCtrl,
  noteTurnStarted,
  registerTurn,
  registerTurnFromRecord,
  shouldSettleTurnFromStream,
  startTurnStream as startTurnStreamCtrl,
  settleTurn as settleTurnCtrl,
  stopOwnedInteractiveStreams,
  tryReattachActiveTurn,
} from "$lib/chat/streamLifecycleController";
import {
  clearWorkerSynthesisFailure,
  ensureWorkerFollowUpBubble,
  hasPendingWorkerSynthesis,
  isRelevantSession,
  linkWorkerFromStream,
  noteWorkerSynthesisFailure,
  onWorkerCardDetail,
  pendingWorkerSynthesisIds,
  recoverPendingWorkerSyntheses,
  resolveTurnSessionId,
  retryWorkerSynthesis,
  syncWorkerLaneFromCards,
  workerLinkForTurn,
} from "$lib/chat/workerLaneController";
import {
  clearBrowserChallenge,
  clearBudgetAlert,
  clearPermissionAlert,
  clearSecretAlert,
  handleBrowserChallenge,
  handleBrowserNavigated,
  handlePermissionRequest,
  handleSecretRequest,
  hasPendingBudgetApproval,
  noteBudgetResolved,
  notePermissionResolved,
  pendingBudgetApprovals,
} from "$lib/chat/turnSideEffectsAdapter";
import {
  attachDroppedFiles,
  attachDroppedPaths,
  attachFilesFromPicker,
  clearPendingMedia,
  removePendingMedia,
} from "$lib/chat/mediaAttachController";

export class ChatStore implements ChatStoreHost {
  askHydrationInFlight = new Set<string>();
  sessionId = $state(loadSessionId());
  messages = $state<ChatMessage[]>([]);
  draft = $state(loadDraftForSession(loadSessionId()));
  vaultNoteContext = $state<VaultNoteContextScope | null>(null);
  pinVaultNoteContext = $state(false);
  scriptWorkbenchContext = $state<ScriptWorkbenchContextScope | null>(null);
  pinScriptWorkbenchContext = $state(false);
  pendingMediaRefs = $state<MediaRef[]>([]);
  pendingMediaUploading = $state(false);
  backgroundActivity = $state(0);
  streamError = $state<string | null>(null);
  sessions = $state<SessionSummary[]>([]);
  sessionListQuery = $state("");
  sessionsError = $state<string | null>(null);
  sessionsRefreshing = $state(false);
  pinnedIds = $state<string[]>(loadPinnedIds());
  historyLoading = $state(true);
  sessionPristine = $state(false);
  historyNotice = $state<string | null>(null);
  askHandoffNotice = $state<string | null>(null);
  promotedAskIds = loadPromotedAskIds();
  budgetAlert = $state<PendingBudgetApproval | null>(null);
  permissionAlert = $state<PendingAgentPermission | null>(null);
  secretAlert = $state<PendingAgentSecret | null>(null);
  browserChallenge = $state<PendingBrowserChallenge | null>(null);
  activeTurnId = $state<string | null>(null);
  contextUsage = $state<ContextUsageReport | null>(null);
  contextUsagePanelOpen = $state(false);
  turns = $state<Map<string, TurnTicketState>>(new Map());
  workers = $state<Map<string, WorkerLink>>(new Map());
  assistantId: string | null = null;
  transcriptEpoch = 0;
  sessionsFetchedAt = 0;
  sessionsRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  sessionsRefreshInFlight: Promise<void> | null = null;
  sessionBootstrapInFlight: Promise<void> | null = null;
  sessionsRefreshDesiredQuery: string | null = null;
  streamOwners = new Map<string, StreamOwner>();
  lastSeqByTurn = new Map<string, number>();
  contentRevealTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private draftPersistTimer: ReturnType<typeof setTimeout> | null = null;
  streamRole: "owner" | "observer" = "owner";
  recentlySettledTurns = new Map<string, number>();
  terminalReconcileTimers = new Map<string, ReturnType<typeof setTimeout>>();
  sessionRuntimes = new Map<string, ChatSessionRuntime>();
  messageIndexes = new Map<string, ChatMessageIndexes>();
  runtimeRevision = $state(0);
  private streamEventPump = new StreamEventPump((target) => {
    this.applyPumpedStreamEvent(target);
  });
  private multiLiveBootstrapped = false;
  private streamApplyPrincipalId: string | null = null;

  setStreamRole(role: "owner" | "observer") {
    this.streamRole = role;
  }

  bootstrapMultiLive(sessionIds?: string[]) {
    if (this.multiLiveBootstrapped) return;
    this.multiLiveBootstrapped = true;
    chatStreamPool.setMaxLive(MAX_SHELL_PANES);
    const principal = this.sessionId.trim();
    const ordered: string[] = [];
    const seen = new Set<string>();
    const push = (id: string) => {
      const trimmed = id.trim();
      if (!trimmed || seen.has(trimmed)) return;
      seen.add(trimmed);
      ordered.push(trimmed);
    };
    push(principal);
    for (const id of sessionIds ?? []) push(id);
    for (const id of ordered) chatStreamPool.acquire(id);
    for (const id of ordered) {
      if (id !== principal) void this.warmBackgroundSession(id);
    }
  }

  async warmBackgroundSession(sessionId: string) {
    return warmBackgroundSession(this, sessionId);
  }

  get focusedSessionId(): string {
    return this.streamApplyPrincipalId ?? this.sessionId;
  }

  private selectorSnapshot() {
    return {
      sessionId: this.sessionId,
      focusedSessionId: this.focusedSessionId,
      streamApplyPrincipalId: this.streamApplyPrincipalId,
      messages: this.messages,
      draft: this.draft,
      streamError: this.streamError,
      historyLoading: this.historyLoading,
      runtimes: this.sessionRuntimes,
    };
  }

  private fieldsMatchFocused(): boolean {
    return this.streamApplyPrincipalId == null;
  }

  messagesFor(sessionId: string): ChatMessage[] {
    void this.runtimeRevision;
    return selectMessagesFor(this.selectorSnapshot(), sessionId);
  }

  draftFor(sessionId: string): string {
    void this.runtimeRevision;
    return selectDraftFor(this.selectorSnapshot(), sessionId);
  }

  streamErrorFor(sessionId: string): string | null {
    void this.runtimeRevision;
    return selectStreamErrorFor(this.selectorSnapshot(), sessionId);
  }

  clearStreamError(sessionId?: string) {
    const trimmed = sessionId?.trim();
    if (!trimmed || trimmed === this.sessionId) this.streamError = null;
    if (trimmed) {
      const runtime = this.sessionRuntimes.get(trimmed);
      if (runtime) runtime.streamError = null;
    }
    this.runtimeRevision += 1;
  }

  historyLoadingFor(sessionId: string): boolean {
    void this.runtimeRevision;
    const trimmed = sessionId.trim();
    if (!trimmed) return false;
    if (this.fieldsMatchFocused() && trimmed === this.sessionId) return this.historyLoading;
    if (this.streamApplyPrincipalId && trimmed === this.streamApplyPrincipalId) {
      return this.sessionRuntimes.get(trimmed)?.historyLoading ?? false;
    }
    if (trimmed === this.sessionId) return this.historyLoading;
    return this.sessionRuntimes.get(trimmed)?.historyLoading ?? false;
  }

  snapshotFocusedRuntime(): ChatSessionRuntime {
    return {
      sessionId: this.sessionId,
      messages: this.messages,
      draft: this.draft,
      streamError: this.streamError,
      historyLoading: this.historyLoading,
      sessionPristine: this.sessionPristine,
      historyNotice: this.historyNotice,
      secretAlert: this.secretAlert,
      activeTurnId: this.activeTurnId,
      turns: this.turns,
      workers: this.workers,
      assistantId: this.assistantId,
      transcriptEpoch: this.transcriptEpoch,
      lastSeqByTurn: this.lastSeqByTurn,
      backgroundActivity: this.backgroundActivity,
    };
  }

  stashFocusedRuntime() {
    const snap = this.snapshotFocusedRuntime();
    this.sessionRuntimes.set(snap.sessionId, snap);
  }

  loadRuntimeIntoFocused(runtime: ChatSessionRuntime) {
    this.sessionId = runtime.sessionId;
    this.messages = runtime.messages;
    this.draft = runtime.draft;
    this.streamError = runtime.streamError;
    this.historyLoading = runtime.historyLoading;
    this.sessionPristine = runtime.sessionPristine;
    this.historyNotice = runtime.historyNotice;
    this.secretAlert = runtime.secretAlert;
    this.activeTurnId = runtime.activeTurnId;
    this.turns = runtime.turns;
    this.workers = runtime.workers as typeof this.workers;
    this.assistantId = runtime.assistantId;
    this.transcriptEpoch = runtime.transcriptEpoch;
    this.lastSeqByTurn = runtime.lastSeqByTurn;
    this.backgroundActivity = runtime.backgroundActivity;
  }

  bumpRuntimeRevision() {
    this.runtimeRevision += 1;
  }

  withSessionFields(sessionId: string, fn: () => void) {
    const trimmed = sessionId.trim();
    if (!trimmed || trimmed === this.sessionId) {
      fn();
      this.stashFocusedRuntime();
      return;
    }
    this.stashFocusedRuntime();
    const focusedId = this.sessionId;
    const target =
      this.sessionRuntimes.get(trimmed) ??
      emptySessionRuntime(trimmed, loadDraftForSession(trimmed));
    this.streamApplyPrincipalId = focusedId;
    this.loadRuntimeIntoFocused(target);
    try {
      fn();
      this.stashFocusedRuntime();
    } finally {
      const restore = this.sessionRuntimes.get(focusedId) ?? emptySessionRuntime(focusedId);
      this.loadRuntimeIntoFocused(restore);
      this.streamApplyPrincipalId = null;
      this.bumpRuntimeRevision();
    }
  }

  ownsInteractiveStreams(): boolean {
    return this.streamRole === "owner";
  }

  get composerBlocked(): boolean {
    return false;
  }

  get liveStreamActive(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (this.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
      return true;
    }
    return false;
  }

  hasWorkshopHandoff(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (turn.phase === "workshop_handoff") return true;
    }
    return false;
  }

  activeWorkshopWorkId(): string | null {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (turn.phase === "workshop_handoff") return turn.workspaceCardId ?? null;
    }
    return null;
  }

  hasLiveInteractiveTurn(): boolean {
    for (const turn of this.turns.values()) {
      if (turn.mode !== "interactive" || turn.terminal) continue;
      if (this.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
      return true;
    }
    return false;
  }

  get hasTurnActivity(): boolean {
    return this.liveStreamActive || this.backgroundActivity > 0;
  }

  get pendingBudgetApprovals(): PendingBudgetApproval[] {
    return pendingBudgetApprovals(this);
  }

  clearBudgetAlert() {
    clearBudgetAlert(this);
  }

  clearPermissionAlert() {
    clearPermissionAlert(this);
  }

  notePermissionResolved(requestId: string) {
    notePermissionResolved(this, requestId);
  }

  handlePermissionRequest(event: InteractiveTurnStreamEvent) {
    handlePermissionRequest(this, event);
  }

  clearSecretAlert() {
    clearSecretAlert(this);
  }

  handleSecretRequest(event: InteractiveTurnStreamEvent) {
    handleSecretRequest(this, event);
  }

  clearBrowserChallenge(sessionId?: string) {
    clearBrowserChallenge(this, sessionId);
  }

  handleBrowserChallenge(event: InteractiveTurnStreamEvent) {
    handleBrowserChallenge(this, event);
  }

  handleBrowserNavigated(event: InteractiveTurnStreamEvent) {
    handleBrowserNavigated(this, event);
  }

  workCardIdForTurn(turnId: string): string | null {
    const cardId = this.turns.get(turnId)?.workspaceCardId?.trim();
    return cardId || null;
  }

  hasPendingBudgetApproval(requestId: string): boolean {
    return hasPendingBudgetApproval(this, requestId);
  }

  noteBudgetResolved(requestId: string) {
    noteBudgetResolved(this, requestId);
  }

  get isStreaming(): boolean {
    return this.liveStreamActive;
  }

  isPinned(sessionId: string): boolean {
    return isPinned(this, sessionId);
  }

  currentSessionLabel(): string {
    return currentSessionLabel(this);
  }

  togglePin(sessionId: string) {
    togglePin(this, sessionId);
  }

  async renameSession(sessionId: string, displayName: string): Promise<void> {
    return renameSession(this, sessionId, displayName);
  }

  async deleteSession(sessionId: string, options?: { purgeMemory?: boolean }) {
    return deleteSessionCtrl(this, sessionId, options);
  }

  async refreshSessions(options?: { force?: boolean; q?: string }) {
    return refreshSessions(this, options);
  }

  scheduleSessionsRefresh() {
    scheduleSessionsRefresh(this);
  }

  async newSession(options?: { shellContext?: { desktopId: string; groupId: string } }) {
    return newSessionCtrl(this, options);
  }

  async newSharedRoom(options?: { displayName?: string; memberProfileIds?: string[] }) {
    return newSharedRoom(this, options);
  }

  async forkFromEntry(message: ChatMessage, options?: { includeDraft?: boolean }) {
    return forkSessionFromEntry(this, message, options);
  }

  async ensureSessionHydrated(options?: { notice?: boolean }) {
    return ensureSessionHydrated(this, options);
  }

  sanitizeTranscript() {
    const deduped = dedupeMessagesById(this.messages);
    if (deduped.length !== this.messages.length) this.messages = deduped;
  }

  async reconcileOnResume(options?: { notice?: boolean }, cards: WorkCard[] = []) {
    return reconcileOnResume(this, options, cards);
  }

  async reloadCurrentSession(options?: { notice?: boolean }) {
    return reloadCurrentSession(this, options);
  }

  async switchSession(sessionId: string) {
    return switchSession(this, sessionId);
  }

  async onSessionDemoted(sessionId: string) {
    const trimmed = sessionId.trim();
    if (!trimmed) return;
    await detachStreamsForSession(this, trimmed);
  }

  clearHistoryNotice() {
    this.historyNotice = null;
  }

  clearAskHandoffNotice() {
    this.askHandoffNotice = null;
  }

  noteTurnStarted(turnId: string) {
    noteTurnStarted(this, turnId);
  }

  registerTurn(ticket: TurnTicketResponse, messageId: string | null) {
    registerTurn(this, ticket, messageId);
  }

  beginTurn(
    userContent: string,
    ticket: TurnTicketResponse,
    mediaRefs: MediaRef[] = [],
    speakerProfileId?: string | null,
  ) {
    beginTurnCtrl(this, userContent, ticket, mediaRefs, speakerProfileId);
  }

  registerTurnFromRecord(record: TurnTicketRecord, messageId: string | null) {
    registerTurnFromRecord(this, record, messageId);
  }

  clearActiveTurn() {
    clearActiveTurn(this);
  }

  async startTurnStream(turnId: string, sessionId: string, streamUrl: string) {
    await startTurnStreamCtrl(this, turnId, sessionId, streamUrl);
  }

  async tryReattachActiveTurn(cards: WorkCard[] = []): Promise<boolean> {
    return tryReattachActiveTurn(this, cards);
  }

  async tryReattachAskTurns(cards: WorkCard[]): Promise<boolean> {
    return this.tryReattachActiveTurn(cards);
  }

  clearOrphanedInteractiveTurns() {
    clearOrphanedInteractiveTurns(this);
  }

  async stopOwnedInteractiveStreams(): Promise<void> {
    await stopOwnedInteractiveStreams(this);
  }

  async cancelActiveTurn(): Promise<void> {
    return cancelActiveTurn(this);
  }

  noteBackgroundSettled(count = 1) {
    noteBackgroundSettled(this, count);
  }

  noteAskTurnSettled(jobId: string) {
    noteAskTurnSettled(this, jobId);
  }

  promoteAskToChat(jobId: string) {
    promoteAskToChat(this, jobId);
  }

  async hydrateAskThreads(cards: WorkCard[]) {
    return hydrateAskThreads(this, cards);
  }

  isRelevantSession(sessionId: string | null | undefined): boolean {
    return isRelevantSession(this, sessionId);
  }

  resolveTurnSessionId(turnId: string | null | undefined, workspaceCardId?: string | null): string {
    return resolveTurnSessionId(this, turnId, workspaceCardId);
  }

  linkWorkerFromStream(event: InteractiveTurnStreamEvent, messageId: string) {
    linkWorkerFromStream(this, event, messageId);
  }

  onWorkerCardDetail(detail: WorkCardDetail, column: string, previousColumn: string | undefined) {
    onWorkerCardDetail(this, detail, column, previousColumn);
  }

  async recoverPendingWorkerSyntheses(cards: WorkCard[], details: Map<string, WorkCardDetail>) {
    return recoverPendingWorkerSyntheses(this, cards, details);
  }

  pendingWorkerSynthesisIds(): Set<string> {
    return pendingWorkerSynthesisIds(this);
  }

  hasPendingWorkerSynthesis(cardOrWorkId: string): boolean {
    return hasPendingWorkerSynthesis(this, cardOrWorkId);
  }

  noteWorkerSynthesisFailure(workId: string, errorLine: string) {
    noteWorkerSynthesisFailure(this, workId, errorLine);
  }

  clearWorkerSynthesisFailure(workId: string) {
    clearWorkerSynthesisFailure(this, workId);
  }

  async retryWorkerSynthesis(workId: string) {
    return retryWorkerSynthesis(this, workId);
  }

  syncWorkerLaneFromCards(cards: WorkCard[], details: Map<string, WorkCardDetail>) {
    syncWorkerLaneFromCards(this, cards, details);
  }

  workerLinkForTurn(turnId: string): WorkerLink | undefined {
    return workerLinkForTurn(this.workers, turnId);
  }

  ensureWorkerFollowUpBubble(
    workId: string,
    turnId: string | null,
    options?: { statusLine?: string | null; streaming?: boolean },
  ): string {
    return ensureWorkerFollowUpBubble(this, workId, turnId, options);
  }

  shouldSettleTurnFromStream(turnId: string): boolean {
    return shouldSettleTurnFromStream(this, turnId);
  }

  settleTurn(turnId: string) {
    settleTurnCtrl(this, turnId);
  }

  applyStreamEvent(event: TurnStreamEnvelopeV2) {
    const owner = this.streamOwners.get(event.turn_id);
    const targetSession = owner?.sessionId?.trim() || this.sessionId;
    const appliedSeq = this.lastAppliedSeq(targetSession, event.turn_id);
    this.streamEventPump.enqueue({ sessionId: targetSession, event }, appliedSeq);
  }

  private applyPumpedStreamEvent(target: StreamEventTarget) {
    applyPumpedStreamEvent(this, target);
  }

  private lastAppliedSeq(sessionId: string, turnId: string): number {
    if (sessionId === this.sessionId) return this.lastSeqByTurn.get(turnId) ?? 0;
    return this.sessionRuntimes.get(sessionId)?.lastSeqByTurn.get(turnId) ?? 0;
  }

  currentMessageIndexes(): ChatMessageIndexes {
    const cached = this.messageIndexes.get(this.sessionId);
    if (cached?.messages === this.messages) return cached;
    const byId = new Map<string, number>();
    const assistantByTurn = new Map<string, string>();
    for (let index = 0; index < this.messages.length; index += 1) {
      const message = this.messages[index];
      byId.set(message.id, index);
      if (message.role === "assistant" && message.turnId) {
        assistantByTurn.set(message.turnId, message.id);
      }
    }
    const indexes = { messages: this.messages, byId, assistantByTurn };
    this.messageIndexes.set(this.sessionId, indexes);
    return indexes;
  }

  messageIndexForId(messageId: string): number {
    return this.currentMessageIndexes().byId.get(messageId) ?? -1;
  }

  replaceMessageAt(index: number, message: ChatMessage) {
    const indexes = this.currentMessageIndexes();
    const previous = this.messages[index];
    const messages = [...this.messages];
    messages[index] = message;
    this.messages = messages;
    indexes.messages = messages;
    if (previous.id !== message.id) indexes.byId.delete(previous.id);
    indexes.byId.set(message.id, index);
    if (previous.turnId && indexes.assistantByTurn.get(previous.turnId) === previous.id) {
      indexes.assistantByTurn.delete(previous.turnId);
    }
    if (message.role === "assistant" && message.turnId) {
      indexes.assistantByTurn.set(message.turnId, message.id);
    }
  }

  appendMessage(message: ChatMessage) {
    const indexes = this.currentMessageIndexes();
    const messages = [...this.messages, message];
    this.messages = messages;
    indexes.messages = messages;
    indexes.byId.set(message.id, messages.length - 1);
    if (message.role === "assistant" && message.turnId) {
      indexes.assistantByTurn.set(message.turnId, message.id);
    }
  }

  applyStreamEventToMessage(messageId: string, event: InteractiveTurnStreamEvent) {
    applyStreamEventToMessageCtrl(this, messageId, event);
  }

  attachOrphanStream(event: InteractiveTurnStreamEvent) {
    attachOrphanStream(this, event);
  }

  syncTurnFromEvent(event: InteractiveTurnStreamEvent) {
    syncTurnFromEventCtrl(this, event);
  }

  messageIdForTurn(turnId: string): string | null {
    const turn = this.turns.get(turnId);
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink && !workerLink.synthesisDelivered) {
      if (workerLink.synthesisMessageId) return workerLink.synthesisMessageId;
      if (workerLink.messageId) return workerLink.messageId;
    }
    if (turn?.messageId) return turn.messageId;
    if (workerLink?.synthesisMessageId) return workerLink.synthesisMessageId;
    return this.currentMessageIndexes().assistantByTurn.get(turnId) ?? null;
  }

  messageIdForToolStream(turnId: string): string | null {
    const workerLink = this.workerLinkForTurn(turnId);
    if (workerLink && !workerLink.synthesisDelivered) {
      if (workerLink.messageId) return workerLink.messageId;
      return this.ensureWorkerFollowUpBubble(workerLink.workId, turnId, {
        statusLine: "Working in background…",
        streaming: true,
      });
    }
    return this.messageIdForTurn(turnId);
  }

  markMessageFailed(messageId: string, errorLine: string, errorDetail: string | null = null) {
    markMessageFailedCtrl(this, messageId, errorLine, errorDetail);
  }

  finishMessage(messageId: string) {
    finishMessageCtrl(this, messageId);
  }

  finishStream() {
    finishStreamCtrl(this);
  }

  setError(message: string) {
    this.streamError = friendlyUserError(message);
    if (this.assistantId) this.finishMessage(this.assistantId);
  }

  noteStreamFailure(message: string, options?: { recoverable?: boolean }) {
    noteStreamFailureCtrl(this, message, options);
  }

  noteResumeFailure(err: unknown) {
    const detail = err instanceof Error ? err.message : String(err);
    console.warn("[chat] resume reconcile failed:", detail);
  }

  evictStreamOwners(turnIds?: string[]) {
    evictStreamOwners(this, turnIds);
  }

  detachStreamOwner(turnId: string): Promise<void> {
    return detachStreamOwner(this, turnId);
  }

  isDetachedWorkerTurnRecord(record: TurnTicketRecord): boolean {
    return isDetachedWorkerTurnRecord(record);
  }

  isComposerOpenDuringHandoff(turnId: string, phase: string): boolean {
    return isComposerOpenDuringHandoff(this, turnId, phase);
  }

  prefillDraft(text: string) {
    this.draft = text;
    this.scheduleDraftPersist();
  }

  prefillFromVaultNote(scope: VaultNoteContextScope, draft: string, options?: { pin?: boolean }) {
    this.vaultNoteContext = scope;
    this.draft = draft;
    this.pinVaultNoteContext = options?.pin ?? false;
    this.scheduleDraftPersist();
  }

  clearVaultNoteContext() {
    this.vaultNoteContext = null;
    this.pinVaultNoteContext = false;
  }

  syncScriptWorkbenchContext(scope: ScriptWorkbenchContextScope | null) {
    this.scriptWorkbenchContext = scope;
  }

  prefillFromScriptWorkbench(
    scope: ScriptWorkbenchContextScope,
    draft: string,
    options?: { pin?: boolean },
  ) {
    this.scriptWorkbenchContext = scope;
    this.draft = draft;
    this.pinScriptWorkbenchContext = options?.pin ?? false;
    this.scheduleDraftPersist();
  }

  clearScriptWorkbenchContext() {
    this.scriptWorkbenchContext = null;
    this.pinScriptWorkbenchContext = false;
  }

  clearPendingMedia() {
    clearPendingMedia(this);
  }

  scheduleDraftPersist() {
    if (this.draftPersistTimer) clearTimeout(this.draftPersistTimer);
    this.draftPersistTimer = setTimeout(() => {
      this.draftPersistTimer = null;
      this.flushDraftPersist();
    }, DRAFT_PERSIST_DEBOUNCE_MS);
  }

  flushDraftPersist() {
    if (this.draftPersistTimer) {
      clearTimeout(this.draftPersistTimer);
      this.draftPersistTimer = null;
    }
    persistDraftForSession(this.sessionId, this.draft);
  }

  clearComposerDraft() {
    this.draft = "";
    clearDraftForSession(this.sessionId);
  }

  removePendingMedia(mediaId: string) {
    removePendingMedia(this, mediaId);
  }

  async attachFilesFromPicker() {
    await attachFilesFromPicker(this);
  }

  async attachDroppedFiles(files: File[]) {
    await attachDroppedFiles(this, files);
  }

  async attachDroppedPaths(paths: string[]) {
    await attachDroppedPaths(this, paths);
  }
}

export const chat = new ChatStore();
