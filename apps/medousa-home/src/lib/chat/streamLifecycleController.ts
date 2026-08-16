/**
 * One owner for interactive stream attach: start / stop / reattach / cancel.
 * ChatPanel still calls ChatStore.beginTurn + startTurnStream; those façades
 * land here. Transcript body apply goes through `$lib/stream/transcriptReducer`.
 */

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
import { mergeTranscript } from "$lib/utils/mergeTranscript";
import { shouldReattachTurnRecord } from "$lib/utils/streamOwnership";
import { streamPathWithSince } from "$lib/stream/reconnect";
import { friendlyUserError } from "$lib/utils/normieErrors";
import { beginTurnMessages, turnStateFromTicket } from "$lib/chat/turnController";
import { mapTurns } from "$lib/chat/sessionController";
import { workerLinkForTurn } from "$lib/chat/workerLaneController";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";

const TERMINAL_RECONCILE_DELAY_MS = 2_000;
const RECENTLY_SETTLED_TTL_MS = 30_000;

export function noteTurnStarted(host: ChatStoreHost, turnId: string) {
  host.activeTurnId = turnId;
}

export function registerTurn(
  host: ChatStoreHost,
  ticket: TurnTicketResponse,
  messageId: string | null,
) {
  host.activeTurnId = ticket.mode === "interactive" ? ticket.turn_id : host.activeTurnId;
  const next = new Map(host.turns);
  next.set(ticket.turn_id, turnStateFromTicket(ticket, messageId));
  host.turns = next;
}

export function beginTurn(
  host: ChatStoreHost,
  userContent: string,
  ticket: TurnTicketResponse,
  mediaRefs: MediaRef[] = [],
  speakerProfileId?: string | null,
) {
  host.sessionPristine = false;
  host.transcriptEpoch += 1;
  host.historyNotice = null;
  host.askHandoffNotice = null;
  const assistantId = crypto.randomUUID();
  host.messages = [
    ...host.messages,
    ...beginTurnMessages({
      userContent,
      ticket,
      mediaRefs,
      speakerProfileId,
      userMessageId: crypto.randomUUID(),
      assistantId,
    }),
  ];
  registerTurn(host, ticket, assistantId);
  if (ticket.mode === "interactive") {
    host.assistantId = assistantId;
    host.activeTurnId = ticket.turn_id;
  } else {
    host.backgroundActivity += 1;
    host.askHandoffNotice = "Ask started";
  }
  host.streamError = null;
}

export function registerTurnFromRecord(
  host: ChatStoreHost,
  record: TurnTicketRecord,
  messageId: string | null,
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
  await startInteractiveStream(streamUrl);
  markStreamOwner(host, turnId, sessionId, streamUrl);
}

export async function tryReattachActiveTurn(
  host: ChatStoreHost,
  cards: WorkCard[] = [],
): Promise<boolean> {
  if (host.streamRole === "observer") return false;
  const sessionId = host.sessionId.trim();
  if (!sessionId) return false;

  await pruneStreamOwnership(host);

  try {
    const targets: TurnTicketRecord[] = [];
    const response = await listSessionTurns(sessionId, true);
    if (response.turns.length === 0) {
      const legacy = await getActiveSessionTurn(sessionId);
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
      if (await attachTurnStream(host, record)) {
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

async function attachTurnStream(host: ChatStoreHost, record: TurnTicketRecord): Promise<boolean> {
  if (host.streamRole === "observer") return false;
  if (!shouldReattachTurnRecord(record, reattachContextFor(host, record))) {
    return false;
  }

  if (host.streamOwners.has(record.turn_id)) {
    await detachStreamOwner(host, record.turn_id);
  }

  let messageId = host.messages.find(
    (message) => message.turnId === record.turn_id && message.role === "assistant",
  )?.id;

  if (!messageId && !record.composer_handoff) {
    messageId = crypto.randomUUID();
    const lane = record.mode === "background" ? ("ask" as const) : ("chat" as const);
    const askJobId =
      record.mode === "background" ? (record.workspace_card_id ?? record.turn_id) : null;
    host.messages = [
      ...host.messages,
      {
        id: messageId,
        role: "assistant",
        content: "",
        streaming: true,
        turnId: record.turn_id,
        lane,
        askJobId,
      },
    ];
    if (record.mode === "interactive") {
      host.assistantId = messageId;
    }
  } else if (messageId && record.mode === "interactive") {
    host.assistantId = messageId;
  }

  registerTurnFromRecord(host, record, messageId ?? null);
  if (record.composer_handoff && record.mode === "interactive") {
    host.backgroundActivity = Math.max(host.backgroundActivity, 1);
  } else if (record.mode === "background") {
    host.backgroundActivity = Math.max(host.backgroundActivity, 1);
  }

  await startInteractiveStream(streamUrlWithSince(host, record.stream_url, record.turn_id));
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
  host.lastSeqByTurn.delete(turnId);
  markRecentlySettled(host, turnId);
  scheduleTerminalHistoryReconcile(host, turnId);
  void detachStreamOwner(host, turnId);
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
  );
  if (!needsMerge) return;

  const epoch = host.transcriptEpoch;
  try {
    const history = await getSessionHistory(sessionId);
    if (epoch !== host.transcriptEpoch) return;
    const daemonMessages = mapTurns(history.turns, { sessionId });
    host.messages = mergeTranscript(host.messages, daemonMessages);
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

