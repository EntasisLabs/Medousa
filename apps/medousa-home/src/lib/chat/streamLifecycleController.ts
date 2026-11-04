/**
 * One owner for interactive stream attach: start / stop / reattach / cancel.
 * ChatPanel still calls ChatStore.beginTurn + startTurnStream; those façades
 * land here. Transcript body authority is ChatCore (snapshot → paint).
 */

import { externalConversationBinding } from "$lib/utils/externalConversationSession";
import {
  cancelActiveSessionTurn,
  getActiveSessionTurn,
  getSessionHistory,
  listSessionTurns,
  startInteractiveStream,
  stopInteractiveStreamTurn,
} from "$lib/daemon";
import type { WorkCard } from "$lib/types/workspace";
import type { TurnTicketRecord, TurnTicketResponse } from "$lib/types/session";
import { isAskJobId, askSessionId } from "$lib/types/askJob";
import type { MediaRef } from "$lib/types/media";
import { stageWhisperAfterFinish } from "$lib/utils/turnInterimDisplay";
import { isEngineTelemetryText } from "$lib/utils/chatStreamDisplay";
import { isReusableAgentStream, shouldReattachTurnRecord } from "$lib/utils/streamOwnership";
import { streamPathWithSince } from "$lib/stream/reconnect";
import { friendlyUserError } from "$lib/utils/normieErrors";
import { randomUuid } from "$lib/utils/randomUuid";
import { turnStateFromTicket } from "$lib/chat/turnController";
import { TRANSCRIPT_PAGE_SIZE } from "$lib/chat/sessionController";
import { workerLinkForTurn } from "$lib/chat/workerLaneController";
import { narration } from "$lib/stores/narration.svelte";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import {
  applyHistoryToSession,
  beginChatTurn,
  openChatSession,
  registerChatTurn,
  settleChatTurn,
} from "$lib/appChat/client";
import { paintHostFromSnapshot } from "$lib/appChat/paintFromSnapshot";
import {
  applySnapshotTurns,
  applySnapshotWorkers,
} from "$lib/appChat/toChatMessage";
import { chatMessageToItem } from "$lib/appChat/fromChatMessage";
import { chatMediaAttachmentsFromRefs } from "$lib/utils/chatMediaUpload";

const TERMINAL_RECONCILE_DELAY_MS = 2_000;
const RECENTLY_SETTLED_TTL_MS = 30_000;

export function noteTurnStarted(host: ChatStoreHost, turnId: string) {
  host.activeTurnId = turnId;
}

export function registerTurn(
  host: ChatStoreHost,
  ticket: TurnTicketResponse,
  messageId: string | null,
  options?: { resetAssistantContent?: boolean },
) {
  const sessionId = ticket.session_id?.trim() || host.sessionId;
  host.activeTurnId = ticket.mode === "interactive" ? ticket.turn_id : host.activeTurnId;
  // Sync chrome only — paint from RegisterTurn is the turn-map authority.
  // streamOwners stay host-local (SSE attach) and overlay streamAttached on paint.
  const next = new Map(host.turns);
  next.set(ticket.turn_id, turnStateFromTicket(ticket, messageId));
  host.turns = next;

  const authority =
    host.messages.find((message) => message.transcript?.authorityId)?.transcript
      ?.authorityId ?? `auth_${"0".repeat(64)}`;
  const epoch = host.transcriptEpoch;
  void openChatSession(sessionId, authority)
    .then(() =>
      registerChatTurn(
        sessionId,
        {
          turnId: ticket.turn_id,
          mode: ticket.mode,
          phase: ticket.phase || "streaming",
          messageId,
          workspaceCardId: ticket.workspace_card_id ?? null,
          reusableStream: isReusableAgentStream(ticket.stream_url),
          budgetRequestId: null,
          requestedRounds: null,
        },
        { resetAssistantContent: options?.resetAssistantContent ?? false },
      ),
    )
    .then((snapshot) => {
      host.withSessionFields(sessionId, () => {
        if (epoch !== host.transcriptEpoch) return;
        const reset = options?.resetAssistantContent ?? false;
        const existing =
          (messageId
            ? host.messages.find((message) => message.id === messageId)
            : undefined) ??
          host.messages.find(
            (message) =>
              message.role === "assistant" && message.turnId === ticket.turn_id,
          );
        const hasLiveBody = Boolean(
          existing &&
            (existing.content?.trim() ||
              (existing.segments?.length ?? 0) > 0 ||
              (existing.toolRuns?.length ?? 0) > 0 ||
              existing.reasoning?.trim()),
        );
        if (hasLiveBody && !reset) {
          // Live stream already projected body/tools — sync turn/worker facts only
          // so a late RegisterTurn paint cannot wipe in-flight segments / bindings.
          const paintedTurns = applySnapshotTurns(snapshot, host.streamOwners);
          for (const [turnId, prev] of host.turns) {
            const next = paintedTurns.get(turnId);
            if (!next || next.messageId || !prev.messageId) continue;
            // Stale RegisterTurn snapshots may still carry null messageId.
            paintedTurns.set(turnId, { ...next, messageId: prev.messageId });
          }
          host.turns = paintedTurns;
          host.workers = applySnapshotWorkers(host.workers, snapshot, sessionId);
          if (existing && ticket.mode === "interactive") {
            host.assistantId = existing.id;
          }
          return;
        }
        paintHostFromSnapshot(host, sessionId, snapshot, {
          liveTurnId: ticket.mode === "interactive" ? ticket.turn_id : null,
          preferredAssistantId: messageId ?? existing?.id ?? null,
        });
      });
    })
    .catch(() => {});
}

export async function beginTurn(
  host: ChatStoreHost,
  userContent: string,
  ticket: TurnTicketResponse,
  mediaRefs: MediaRef[] = [],
  speakerProfileId?: string | null,
): Promise<void> {
  const sessionId = ticket.session_id?.trim() || host.sessionId;
  const userMessageId = randomUuid();
  const assistantId = randomUuid();
  const isAsk = ticket.mode === "background";
  const askJobId = ticket.workspace_card_id ?? ticket.turn_id;
  const lane = isAsk ? ("ask" as const) : ("chat" as const);
  const statusLine = ticket.mode === "background" ? "Background turn started" : null;
  const mediaAttachments =
    mediaRefs.length > 0 ? chatMediaAttachmentsFromRefs(mediaRefs) : null;

  let authority = `auth_${"0".repeat(64)}`;
  host.withSessionFields(sessionId, () => {
    narration.stop();
    host.sessionPristine = false;
    host.transcriptEpoch += 1;
    host.historyNotice = null;
    host.askHandoffNotice = null;
    host.streamError = null;
    authority =
      host.messages.find((message) => message.transcript?.authorityId)?.transcript
        ?.authorityId ?? authority;
    if (ticket.mode === "interactive") {
      host.assistantId = assistantId;
      host.activeTurnId = ticket.turn_id;
    } else {
      host.backgroundActivity += 1;
      host.askHandoffNotice = "Ask started";
    }
  });

  try {
    await openChatSession(sessionId, authority);
    const snapshot = await beginChatTurn(sessionId, {
      turnId: ticket.turn_id,
      userItemId: userMessageId,
      assistantItemId: assistantId,
      userText: userContent,
      createdAt: ticket.accepted_at_utc ?? null,
      lane,
      askJobId: isAsk ? askJobId : null,
      statusLine,
      speakerProfileId: speakerProfileId?.trim() || null,
      mediaAttachments,
      mode: ticket.mode,
      workspaceCardId: ticket.workspace_card_id ?? (isAsk ? askJobId : null),
      reusableStream: isReusableAgentStream(ticket.stream_url),
    });
    host.withSessionFields(sessionId, () => {
      paintHostFromSnapshot(host, sessionId, snapshot, {
        liveTurnId: ticket.mode === "interactive" ? ticket.turn_id : null,
        preferredAssistantId: assistantId,
      });
    });
  } catch {
    // Core owns transcript shells — leave host unchanged when IPC is unavailable.
    // Vitest installs the app-chat dispatch backend; reconnect races retry via reattach.
  }
}

export function registerTurnFromRecord(
  host: ChatStoreHost,
  record: TurnTicketRecord,
  messageId: string | null,
  options?: { resetAssistantContent?: boolean },
) {
  registerTurn(
    host,
    {
      turn_id: record.turn_id,
      session_id: record.session_id,
      mode: record.mode,
      phase: record.phase,
      accepted_at_utc: record.started_at,
      stream_url: record.stream_url,
      stream_ready: true,
      workspace_card_id: record.workspace_card_id ?? null,
    },
    messageId,
    options,
  );
}

export function clearActiveTurn(host: ChatStoreHost) {
  host.activeTurnId = null;
}

export async function startTurnStream(
  host: ChatStoreHost,
  turnId: string,
  sessionId: string,
  streamUrl: string,
) {
  await startInteractiveStream(streamUrlWithSince(host, streamUrl, turnId));
  markStreamOwner(host, turnId, sessionId, streamUrl);
}

export async function tryReattachActiveTurn(
  host: ChatStoreHost,
  cards: WorkCard[] = [],
): Promise<boolean> {
  if (host.streamRole === "observer") return false;
  const sessionId = host.sessionId.trim();
  if (!sessionId || externalConversationBinding(sessionId)) return false;

  const workshopEpoch = host.workshopEpoch;
  const transcriptEpoch = host.transcriptEpoch;
  const current = () => host.workshopEpoch === workshopEpoch &&
    host.transcriptEpoch === transcriptEpoch && host.sessionId.trim() === sessionId &&
    !host.history?.isReleased(sessionId);
  await pruneStreamOwnership(host);
  if (!current()) return false;

  try {
    const targets: TurnTicketRecord[] = [];
    const response = await listSessionTurns(sessionId, true);
    if (!current()) return false;
    if (response.turns.length === 0) {
      const legacy = await getActiveSessionTurn(sessionId);
      if (!current()) return false;
      if (!legacy?.active || !legacy.turn) {
        host.activeTurnId = null;
        clearOrphanedInteractiveTurns(host);
        return false;
      }
      response.turns.push({
        turn_id: legacy.turn.turn_id,
        session_id: legacy.turn.session_id,
        mode: "interactive",
        phase: "streaming",
        stream_url: legacy.turn.stream_url,
        prompt_preview: "",
        workspace_card_id: null,
        composer_handoff: legacy.turn.composer_handoff,
        started_at: legacy.turn.started_at,
        updated_at: legacy.turn.started_at,
      });
    }
    targets.push(...response.turns);

    for (const card of cards) {
      if (!isAskJobId(card.id)) continue;
      if (host.promotedAskIds.has(card.id)) continue;
      if (card.column === "done" || card.column === "blocked") continue;
      try {
        const askResponse = await listSessionTurns(askSessionId(card.id), true);
        if (!current()) return false;
        targets.push(...askResponse.turns);
      } catch {
        // Best-effort — card may still be queued.
      }
    }

    let attached = false;
    const seen = new Set<string>();
    for (const record of targets) {
      if (seen.has(record.turn_id)) continue;
      seen.add(record.turn_id);
      if (!current()) return false;
      if (await attachTurnStream(host, record, current)) {
        attached = true;
      }
    }

    await pruneStreamOwnership(host);
    return attached;
  } catch (err) {
    host.noteResumeFailure(err);
    return false;
  }
}

export function clearOrphanedInteractiveTurns(host: ChatStoreHost) {
  const orphanIds: string[] = [];
  for (const [turnId, turn] of host.turns) {
    if (turn.terminal || turn.mode !== "interactive") continue;
    if (host.isComposerOpenDuringHandoff(turnId, turn.phase)) continue;
    orphanIds.push(turnId);
  }

  for (const turnId of orphanIds) {
    const turn = host.turns.get(turnId);
    const messageId = turn?.messageId?.trim() || null;
    if (messageId) {
      const idx = host.messages.findIndex((message) => message.id === messageId);
      if (idx >= 0) {
        const current = host.messages[idx];
        host.messages = [
          ...host.messages.slice(0, idx),
          {
            ...current,
            streaming: false,
            failed: false,
            errorLine: null,
            errorDetail: null,
            answerState: current.content.trim() ? current.answerState : null,
            phase: null,
            statusLine: null,
          },
          ...host.messages.slice(idx + 1),
        ];
      }
      if (host.assistantId === messageId) {
        host.assistantId = null;
      }
    }
    settleTurn(host, turnId);
  }

  for (const message of host.messages) {
    if (
      !message.streaming ||
      message.role !== "assistant" ||
      message.lane === "worker" ||
      message.phase === "budget_blocked"
    ) {
      continue;
    }
    const turnId = message.turnId?.trim();
    if (turnId && host.turns.has(turnId)) continue;
    finishMessage(host, message.id);
  }

  if (orphanIds.length > 0) {
    host.streamError = null;
  }
}

function reattachContextFor(host: ChatStoreHost, record: TurnTicketRecord) {
  const assistant = host.messages.find(
    (message) => message.turnId === record.turn_id && message.role === "assistant",
  );
  return {
    principalSessionId: host.sessionId,
    isRelevantSession: (sessionId: string | null | undefined) =>
      host.isRelevantSession(sessionId),
    isDetachedWorkerTurn: (ticket: TurnTicketRecord) =>
      host.isDetachedWorkerTurnRecord(ticket),
    localTurn: host.turns.get(record.turn_id),
    hasAssistantMessage: assistant != null,
    assistantStreaming: assistant?.streaming ?? false,
  };
}

export function markStreamOwner(
  host: ChatStoreHost,
  turnId: string,
  sessionId: string,
  streamUrl: string,
) {
  host.streamOwners.set(turnId, { turnId, sessionId, streamUrl });
}

function streamUrlWithSince(host: ChatStoreHost, streamUrl: string, turnId: string): string {
  const lastSeq = host.lastSeqByTurn.get(turnId) ?? 0;
  return streamPathWithSince(streamUrl, lastSeq);
}

export async function detachStreamOwner(host: ChatStoreHost, turnId: string) {
  if (!host.streamOwners.delete(turnId)) return;
  try {
    await stopInteractiveStreamTurn(turnId);
  } catch {
    // Best-effort detach.
  }
}

async function clearStreamOwnership(host: ChatStoreHost) {
  const turnIds = [...host.streamOwners.keys()];
  host.streamOwners.clear();
  await Promise.all(
    turnIds.map((turnId) => stopInteractiveStreamTurn(turnId).catch(() => undefined)),
  );
}

export async function stopOwnedInteractiveStreams(host: ChatStoreHost): Promise<void> {
  await clearStreamOwnership(host);
}

export async function detachStreamsForSession(host: ChatStoreHost, sessionId: string) {
  const turnIds = [...host.streamOwners.entries()]
    .filter(([, owner]) => owner.sessionId === sessionId)
    .map(([turnId]) => turnId);
  await Promise.all(turnIds.map((turnId) => detachStreamOwner(host, turnId)));
}

async function pruneStreamOwnership(host: ChatStoreHost) {
  for (const [turnId] of host.streamOwners) {
    const turn = host.turns.get(turnId);
    if (!turn || turn.terminal) {
      await detachStreamOwner(host, turnId);
      continue;
    }
    if (turn.phase === "worker_handoff" && turn.mode === "interactive") {
      await detachStreamOwner(host, turnId);
      continue;
    }
    if (turn.phase === "workshop_handoff" && turn.mode === "interactive") {
      const workerLink = workerLinkForTurn(host.workers, turnId);
      if (workerLink?.synthesisDelivered) {
        await detachStreamOwner(host, turnId);
      }
    }
  }
}

async function attachTurnStream(host: ChatStoreHost, record: TurnTicketRecord, current: () => boolean): Promise<boolean> {
  if (host.streamRole === "observer") return false;
  if (!shouldReattachTurnRecord(record, reattachContextFor(host, record))) {
    return false;
  }

  if (host.streamOwners.has(record.turn_id)) {
    await detachStreamOwner(host, record.turn_id);
    if (!current()) return false;
  }

  const existingMessageId = host.messages.find(
    (message) => message.turnId === record.turn_id && message.role === "assistant",
  )?.id;
  let messageId = existingMessageId;

  // History has no SSE cursor; native streams replay from zero — clear the
  // hydrated body in core before resume so replay does not double-append.
  const replayFromStart =
    Boolean(messageId) &&
    !record.composer_handoff &&
    !host.lastSeqByTurn.has(record.turn_id) &&
    !isReusableAgentStream(record.stream_url);
  const seedShell = !messageId && !record.composer_handoff;

  if (seedShell) {
    messageId = randomUuid();
  }

  const sessionId = record.session_id?.trim() || host.sessionId;
  const authority =
    host.messages.find((message) => message.transcript?.authorityId)?.transcript
      ?.authorityId ?? `auth_${"0".repeat(64)}`;
  const epoch = host.transcriptEpoch;

  if (messageId && !record.composer_handoff) {
    try {
      await openChatSession(sessionId, authority);
      if (!current() || epoch !== host.transcriptEpoch) return false;
      const snapshot = await registerChatTurn(
        sessionId,
        {
          turnId: record.turn_id,
          mode: record.mode,
          phase: record.phase || "streaming",
          messageId,
          workspaceCardId: record.workspace_card_id ?? null,
          reusableStream: isReusableAgentStream(record.stream_url),
          budgetRequestId: null,
          requestedRounds: null,
        },
        { resetAssistantContent: replayFromStart },
      );
      if (!current() || epoch !== host.transcriptEpoch) return false;
      host.withSessionFields(sessionId, () => {
        if (seedShell || replayFromStart) {
          // Core just created or cleared the assistant shell — paint it.
          paintHostFromSnapshot(host, sessionId, snapshot, {
            liveTurnId: record.mode === "interactive" ? record.turn_id : null,
            preferredAssistantId: messageId,
          });
        } else {
          // Live projection already has body/cursor — sync turn/worker facts only.
          host.turns = applySnapshotTurns(snapshot, host.streamOwners);
          host.workers = applySnapshotWorkers(host.workers, snapshot, sessionId);
          host.messages = host.messages.map((message) =>
            message.id === messageId
              ? {
                  ...message,
                  streaming: true,
                  failed: false,
                  errorLine: null,
                  errorDetail: null,
                  phase: record.phase,
                }
              : message,
          );
          if (record.mode === "interactive") host.assistantId = messageId;
        }
      });
    } catch {
      // Core owns assistant shells — leave host unchanged when IPC is unavailable.
    }
  } else {
    registerTurnFromRecord(host, record, messageId ?? null);
  }

  if (record.composer_handoff && record.mode === "interactive") {
    host.backgroundActivity = Math.max(host.backgroundActivity, 1);
  } else if (record.mode === "background") {
    host.backgroundActivity = Math.max(host.backgroundActivity, 1);
  }

  await startInteractiveStream(streamUrlWithSince(host, record.stream_url, record.turn_id));
  if (!current()) {
    await stopInteractiveStreamTurn(record.turn_id).catch(() => undefined);
    return false;
  }
  markStreamOwner(host, record.turn_id, record.session_id, record.stream_url);
  return true;
}

export async function cancelActiveTurn(host: ChatStoreHost): Promise<void> {
  const sessionId = host.sessionId.trim();
  if (!sessionId) return;

  const turnId = host.activeTurnId;

  try {
    await cancelActiveSessionTurn(sessionId);
  } catch {
    // Best-effort — still settle local state below.
  }

  if (turnId) {
    if (host.assistantId) {
      finishMessage(host, host.assistantId);
    }
    settleTurn(host, turnId);
    return;
  }

  const ownedTurnIds = [...host.streamOwners.entries()]
    .filter(([, owner]) => owner.sessionId === sessionId)
    .map(([id]) => id);
  evictStreamOwners(host, ownedTurnIds);
  for (const ownedTurnId of ownedTurnIds) {
    await stopInteractiveStreamTurn(ownedTurnId).catch(() => undefined);
  }
  host.activeTurnId = null;
  host.assistantId = null;
}

export function noteBackgroundSettled(host: ChatStoreHost, count = 1) {
  host.backgroundActivity = Math.max(0, host.backgroundActivity - count);
}

export function noteAskTurnSettled(host: ChatStoreHost, jobId: string) {
  const trimmed = jobId.trim();
  if (!trimmed) return;

  let settledTurn = false;
  for (const [turnId, turn] of host.turns) {
    if (turn.mode !== "background") continue;
    if (turn.workspaceCardId !== trimmed && turnId !== trimmed) continue;
    settleTurn(host, turnId);
    settledTurn = true;
  }

  host.messages = host.messages.map((message) =>
    message.askJobId === trimmed && message.streaming
      ? {
          ...message,
          streaming: false,
          phase: null,
          statusLine: null,
          stageWhisper: stageWhisperAfterFinish(
            message.statusLine,
            message.content,
            message.stageWhisper,
          ),
        }
      : message,
  );
  if (!settledTurn) {
    noteBackgroundSettled(host);
  }
}

export function shouldSettleTurnFromStream(host: ChatStoreHost, turnId: string): boolean {
  const turn = host.turns.get(turnId);
  if (turn?.mode === "background") return false;
  const workerLink = workerLinkForTurn(host.workers, turnId);
  if (workerLink && !workerLink.synthesisDelivered) return false;
  return true;
}

export function settleTurn(host: ChatStoreHost, turnId: string) {
  const turn = host.turns.get(turnId);
  if (!turn) return;
  const sessionId = host.sessionId;
  const reusableStream = turn.reusableStream;
  if (turn.mode === "background" || host.backgroundActivity > 0) {
    host.backgroundActivity = Math.max(0, host.backgroundActivity - 1);
  }
  if (host.activeTurnId === turnId) {
    host.activeTurnId = null;
  }
  if (host.assistantId && turn.messageId === host.assistantId) {
    host.assistantId = null;
  }
  const next = new Map(host.turns);
  next.delete(turnId);
  host.turns = next;
  // Agent-session IDs and their sequence persist across prompts. Keep the
  // cursor so the next subscription cannot replay the previous terminal event.
  if (!reusableStream) host.lastSeqByTurn.delete(turnId);
  markRecentlySettled(host, turnId);
  scheduleTerminalHistoryReconcile(host, turnId);
  void detachStreamOwner(host, turnId);
  const epoch = host.transcriptEpoch;
  void settleChatTurn(sessionId, turnId, {
    reusableStream,
  })
    .then((snapshot) => {
      host.withSessionFields(sessionId, () => {
        if (epoch !== host.transcriptEpoch) return;
        paintHostFromSnapshot(host, sessionId, snapshot);
      });
    })
    .catch(() => {});
}

function markRecentlySettled(host: ChatStoreHost, turnId: string) {
  host.recentlySettledTurns.set(turnId, Date.now());
  for (const [id, settledAt] of host.recentlySettledTurns) {
    if (Date.now() - settledAt > RECENTLY_SETTLED_TTL_MS) {
      host.recentlySettledTurns.delete(id);
    }
  }
}

export function recentlySettledTurnIdSet(host: ChatStoreHost): ReadonlySet<string> {
  const ids = new Set<string>();
  for (const [id, settledAt] of host.recentlySettledTurns) {
    if (Date.now() - settledAt <= RECENTLY_SETTLED_TTL_MS) {
      ids.add(id);
    }
  }
  return ids;
}

export function transcriptTurnIdSet(host: ChatStoreHost): ReadonlySet<string> {
  const ids = new Set<string>();
  for (const message of host.messages) {
    const turnId = message.turnId?.trim();
    if (turnId) ids.add(turnId);
  }
  return ids;
}

function scheduleTerminalHistoryReconcile(host: ChatStoreHost, turnId: string) {
  const trimmed = turnId.trim();
  if (!trimmed) return;
  const existing = host.terminalReconcileTimers.get(trimmed);
  if (existing) clearTimeout(existing);
  host.terminalReconcileTimers.set(
    trimmed,
    setTimeout(() => {
      host.terminalReconcileTimers.delete(trimmed);
      void reconcileTurnFromHistory(host, trimmed);
    }, TERMINAL_RECONCILE_DELAY_MS),
  );
}

async function reconcileTurnFromHistory(host: ChatStoreHost, turnId: string) {
  const sessionId = host.sessionId.trim();
  if (!sessionId) return;

  const assistants = host.messages.filter(
    (message) => message.turnId === turnId && message.role === "assistant",
  );
  if (assistants.length === 0) return;

  const needsMerge = assistants.some(
    (message) =>
      message.streaming ||
      message.failed ||
      !message.content.trim() ||
      isEngineTelemetryText(message.content),
  ) || assistants.some(
    (message) =>
      message.tools?.includes("cognition_image_generate") ||
      message.toolRuns?.some((run) => run.toolName === "cognition_image_generate"),
  );
  if (!needsMerge) return;

  const epoch = host.transcriptEpoch;
  try {
    const history = await getSessionHistory(sessionId, { limit: TRANSCRIPT_PAGE_SIZE });
    if (epoch !== host.transcriptEpoch) return;
    await openChatSession(sessionId, history.authority_id);
    const snapshot = await applyHistoryToSession({
      sessionId,
      history,
      replace: false,
      localItems: host.messages.map(chatMessageToItem),
    });
    if (epoch !== host.transcriptEpoch) return;
    paintHostFromSnapshot(host, sessionId, snapshot);
    host.sanitizeTranscript();
  } catch {
    // Best-effort — manual reload still works.
  }
}


function cancelContentReveal(host: ChatStoreHost, messageId: string) {
  const timer = host.contentRevealTimers.get(messageId);
  if (timer) {
    clearTimeout(timer);
    host.contentRevealTimers.delete(messageId);
  }
}

export function markMessageFailed(
  host: ChatStoreHost,
  messageId: string,
  errorLine: string,
  errorDetail: string | null = null,
) {
  const idx = host.messages.findIndex((message) => message.id === messageId);
  if (idx < 0) return;
  const current = host.messages[idx];
  host.messages = [
    ...host.messages.slice(0, idx),
    {
      ...current,
      streaming: false,
      failed: true,
      errorLine,
      errorDetail,
      answerState: "failed",
      phase: null,
      statusLine: null,
    },
    ...host.messages.slice(idx + 1),
  ];
}

export function finishMessage(host: ChatStoreHost, messageId: string) {
  cancelContentReveal(host, messageId);
  const idx = host.messages.findIndex((m) => m.id === messageId);
  if (idx >= 0) {
    const current = host.messages[idx];
    const next = {
      ...current,
      streaming: false,
      phase: null,
      statusLine: null,
      stageWhisper: stageWhisperAfterFinish(
        current.statusLine,
        current.content,
        current.stageWhisper,
      ),
    };
    host.messages = [...host.messages.slice(0, idx), next, ...host.messages.slice(idx + 1)];
  }
  if (host.assistantId === messageId) {
    host.assistantId = null;
  }
}

export function finishStream(host: ChatStoreHost) {
  if (host.assistantId) {
    finishMessage(host, host.assistantId);
  }
}


export function noteStreamFailure(
  host: ChatStoreHost,
  message: string,
  options?: { recoverable?: boolean },
) {
  const recoverable = options?.recoverable !== false;
  const liveTurn = hasLiveInteractiveTurn(host);
  const messageId =
    host.assistantId ??
    [...host.turns.values()].find((turn) => turn.mode === "interactive" && !turn.terminal)
      ?.messageId ??
    null;

  if (!recoverable && liveTurn && messageId) {
    markMessageFailed(host, messageId, friendlyUserError(message));
    if (host.assistantId === messageId) {
      host.assistantId = null;
    }
  }

  evictStreamOwners(host);

  if (recoverable && !liveTurn) {
    return;
  }

  host.streamError = friendlyUserError(message);
  if (recoverable && liveTurn) {
    return;
  }
  if (host.assistantId) {
    finishMessage(host, host.assistantId);
  }
  for (const [turnId, turn] of host.turns) {
    if (turn.terminal || turn.mode === "background") continue;
    if (
      turn.phase === "budget_blocked" ||
      turn.phase === "worker_handoff" ||
      turn.phase === "workshop_handoff"
    ) {
      continue;
    }
    settleTurn(host, turnId);
  }
}

export function evictStreamOwners(host: ChatStoreHost, turnIds?: string[]) {
  if (turnIds) {
    for (const turnId of turnIds) {
      host.streamOwners.delete(turnId);
    }
    return;
  }
  host.streamOwners.clear();
}

function hasLiveInteractiveTurn(host: ChatStoreHost): boolean {
  for (const turn of host.turns.values()) {
    if (turn.mode !== "interactive" || turn.terminal) continue;
    if (host.isComposerOpenDuringHandoff(turn.turnId, turn.phase)) continue;
    return true;
  }
  return false;
}

export function isDetachedWorkerTurnRecord(record: TurnTicketRecord): boolean {
  const cardId = record.workspace_card_id?.trim();
  if (cardId?.startsWith("work-")) {
    return true;
  }
  if (record.mode === "background" && cardId?.startsWith("medousa-daemon-ask-")) {
    return false;
  }
  return false;
}

export function isComposerOpenDuringHandoff(
  host: ChatStoreHost,
  turnId: string,
  phase: string,
): boolean {
  if (phase === "worker_handoff" || phase === "workshop_handoff" || phase === "budget_blocked") {
    return true;
  }
  const workerLink = workerLinkForTurn(host.workers, turnId);
  return workerLink != null && !workerLink.synthesisDelivered;
}
