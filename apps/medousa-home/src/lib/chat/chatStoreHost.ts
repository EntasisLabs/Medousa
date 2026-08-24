/**
 * Host surface controllers use instead of importing ChatStore (avoids cycles).
 * ChatStore structurally satisfies this; controllers never import the store module.
 */

import type {
  ChatMessage,
  ContextUsageReport,
  InteractiveTurnStreamEvent,
  PendingAgentPermission,
  PendingAgentSecret,
  PendingBrowserChallenge,
  PendingBudgetApproval,
  TurnTicketState,
} from "$lib/types/chat";
import type { WorkCardDetail } from "$lib/types/card";
import type { MediaRef } from "$lib/types/media";
import type { SessionSummary, TurnTicketRecord } from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import type { ChatSessionRuntime, WorkerLink } from "$lib/chat/chatSessionRuntime";
import type { StreamOwner } from "$lib/utils/streamOwnership";

export type ChatMessageIndexes = {
  messages: ChatMessage[];
  byId: Map<string, number>;
  assistantByTurn: Map<string, string>;
};

export type ChatStoreHost = {
  workshopScopeId: string;
  workshopEpoch: number;
  sessionId: string;
  messages: ChatMessage[];
  draft: string;
  pendingMediaRefs: MediaRef[];
  pendingMediaUploading: boolean;
  backgroundActivity: number;
  streamError: string | null;
  sessions: SessionSummary[];
  sessionListQuery: string;
  sessionsError: string | null;
  sessionsRefreshing: boolean;
  pinnedIds: string[];
  historyLoading: boolean;
  sessionPristine: boolean;
  historyNotice: string | null;
  askHandoffNotice: string | null;
  budgetAlert: PendingBudgetApproval | null;
  permissionAlert: PendingAgentPermission | null;
  secretAlert: PendingAgentSecret | null;
  browserChallenge: PendingBrowserChallenge | null;
  activeTurnId: string | null;
  contextUsage: ContextUsageReport | null;
  contextUsagePanelOpen: boolean;
  turns: Map<string, TurnTicketState>;
  workers: Map<string, WorkerLink>;
  streamRole: "owner" | "observer";
  runtimeRevision: number;
  assistantId: string | null;
  transcriptEpoch: number;
  sessionsFetchedAt: number;
  sessionsRefreshTimer: ReturnType<typeof setTimeout> | null;
  sessionsRefreshInFlight: Promise<void> | null;
  sessionBootstrapInFlight: Promise<void> | null;
  sessionsRefreshDesiredQuery: string | null;
  streamOwners: Map<string, StreamOwner>;
  lastSeqByTurn: Map<string, number>;
  contentRevealTimers: Map<string, ReturnType<typeof setTimeout>>;
  recentlySettledTurns: Map<string, number>;
  terminalReconcileTimers: Map<string, ReturnType<typeof setTimeout>>;
  sessionRuntimes: Map<string, ChatSessionRuntime>;
  messageIndexes: Map<string, ChatMessageIndexes>;
  promotedAskIds: Set<string>;
  askHydrationInFlight: Set<string>;

  stashFocusedRuntime(): void;
  loadRuntimeIntoFocused(runtime: ChatSessionRuntime): void;
  bumpRuntimeRevision(): void;
  flushDraftPersist(): void;
  sanitizeTranscript(): void;
  noteResumeFailure(err: unknown): void;
  withSessionFields(sessionId: string, fn: () => void): void;
  replaceMessageAt(index: number, message: ChatMessage): void;
  appendMessage(message: ChatMessage): void;
  messageIndexForId(messageId: string): number;
  messageIdForTurn(turnId: string): string | null;
  messageIdForToolStream(turnId: string): string | null;
  currentMessageIndexes(): ChatMessageIndexes;
  workCardIdForTurn(turnId: string): string | null;
  setError(message: string): void;
  finishMessage(messageId: string): void;
  settleTurn(turnId: string): void;
  markMessageFailed(messageId: string, errorLine: string, errorDetail?: string | null): void;
  tryReattachActiveTurn(cards?: WorkCard[]): Promise<boolean>;
  newSession(options?: { shellContext?: { desktopId: string; groupId: string } }): Promise<void>;
  refreshSessions(options?: { force?: boolean; q?: string }): Promise<void>;
  scheduleSessionsRefresh(): void;
  linkWorkerFromStream(event: InteractiveTurnStreamEvent, messageId: string): void;
  handlePermissionRequest(event: InteractiveTurnStreamEvent): void;
  handleSecretRequest(event: InteractiveTurnStreamEvent): void;
  handleBrowserChallenge(event: InteractiveTurnStreamEvent): void;
  handleBrowserNavigated(event: InteractiveTurnStreamEvent): void;
  clearPermissionAlert(): void;
  clearSecretAlert(): void;
  workerLinkForTurn(turnId: string): WorkerLink | undefined;
  isRelevantSession(sessionId: string | null | undefined): boolean;
  isDetachedWorkerTurnRecord(record: TurnTicketRecord): boolean;
  isComposerOpenDuringHandoff(turnId: string, phase: string): boolean;
  ensureWorkerFollowUpBubble(
    workId: string,
    turnId: string | null,
    options?: { statusLine?: string | null; streaming?: boolean },
  ): string;
  noteBackgroundSettled(count?: number): void;
  detachStreamOwner(turnId: string): Promise<void>;
  applyStreamEventToMessage(messageId: string, event: InteractiveTurnStreamEvent): void;
  attachOrphanStream(event: InteractiveTurnStreamEvent): void;
  syncTurnFromEvent(event: InteractiveTurnStreamEvent): void;
  shouldSettleTurnFromStream(turnId: string): boolean;
  resolveTurnSessionId(turnId: string | null | undefined, workspaceCardId?: string | null): string;
  registerTurnFromRecord(record: TurnTicketRecord, messageId: string | null): void;
};
