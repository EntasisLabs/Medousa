/**
 * Worker-lane bubbles: handoff ack, live status, and synthesis delivery.
 */

import { getSessionHistory } from "$lib/daemon";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";
import { isAskJobId, askJobIdFromSession, askSessionId } from "$lib/types/askJob";
import { workerStatusLineForColumn } from "$lib/utils/workerThreads";
import { randomUuid } from "$lib/utils/randomUuid";
import type { WorkerLink } from "$lib/chat/chatSessionRuntime";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import { TRANSCRIPT_PAGE_SIZE } from "$lib/chat/sessionController";

export function workerLinkForTurn(
  workers: Map<string, WorkerLink>,
  turnId: string,
): WorkerLink | undefined {
  for (const link of workers.values()) {
    if (link.parentTurnId === turnId) return link;
  }
  return undefined;
}

export function isRelevantSession(host: ChatStoreHost, sessionId: string | null | undefined): boolean {
  const trimmed = sessionId?.trim();
  if (!trimmed) return false;
  if (trimmed === host.sessionId) return true;

  for (const link of host.workers.values()) {
    if (link.sessionId === trimmed) return true;
  }

  const jobId = askJobIdFromSession(trimmed);
  if (!jobId) return false;

  if (host.messages.some((message) => message.askJobId === jobId)) {
    return true;
  }

  for (const turn of host.turns.values()) {
    if (turn.workspaceCardId === jobId) return true;
  }

  return false;
}

export function isRelevantWorkerDetail(
  host: ChatStoreHost,
  detail: WorkCardDetail,
  workId: string,
): boolean {
  if (host.workers.has(workId)) return true;

  const parentTurnId = detail.correlation_id?.trim();
  if (parentTurnId) {
    if (host.turns.has(parentTurnId)) return true;
    if (host.messages.some((message) => message.turnId === parentTurnId)) {
      return true;
    }
  }

  const sessionId = detail.session_id?.trim();
  return Boolean(sessionId && isRelevantSession(host, sessionId));
}

export function resolveTurnSessionId(
  host: ChatStoreHost,
  turnId: string | null | undefined,
  workspaceCardId?: string | null,
): string {
  const cardId = workspaceCardId?.trim();
  if (cardId && isAskJobId(cardId)) {
    return askSessionId(cardId);
  }
  if (turnId) {
    const turn = host.turns.get(turnId);
    if (turn?.workspaceCardId && isAskJobId(turn.workspaceCardId)) {
      return askSessionId(turn.workspaceCardId);
    }
    if (turn?.mode === "background" && isAskJobId(turnId)) {
      return askSessionId(turnId);
    }
  }
  return host.sessionId;
}

export function linkWorker(
  host: ChatStoreHost,
  params: {
    workId: string;
    parentTurnId: string | null;
    messageId: string | null;
    sessionId: string;
  },
) {
  const existing = host.workers.get(params.workId);
  const link: WorkerLink = {
    workId: params.workId,
    parentTurnId: params.parentTurnId ?? existing?.parentTurnId ?? null,
    messageId: params.messageId ?? existing?.messageId ?? null,
    synthesisMessageId: existing?.synthesisMessageId ?? null,
    sessionId: params.sessionId,
    synthesisDelivered: existing?.synthesisDelivered ?? false,
  };
  const nextWorkers = new Map(host.workers);
  nextWorkers.set(params.workId, link);
  host.workers = nextWorkers;

  if (params.parentTurnId) {
    const turn = host.turns.get(params.parentTurnId);
    if (turn) {
      const nextTurns = new Map(host.turns);
      nextTurns.set(params.parentTurnId, {
        ...turn,
        workspaceCardId: params.workId,
      });
      host.turns = nextTurns;
    }
  }
}

export function linkWorkerFromStream(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
  messageId: string,
) {
  const workId = event.work_id?.trim();
  if (!workId) return;
  linkWorker(host, {
    workId,
    parentTurnId: event.turn_id,
    messageId,
    sessionId: resolveTurnSessionId(host, event.turn_id),
  });
  const followUpId = ensureWorkerFollowUpBubble(host, workId, event.turn_id, {
    statusLine: "Working in background…",
    streaming: true,
  });
  const link = host.workers.get(workId);
  if (link && link.synthesisMessageId !== followUpId) {
    const nextWorkers = new Map(host.workers);
    nextWorkers.set(workId, { ...link, synthesisMessageId: followUpId });
    host.workers = nextWorkers;
  }
}

export function onWorkerCardDetail(
  host: ChatStoreHost,
  detail: WorkCardDetail,
  column: string,
  previousColumn: string | undefined,
) {
  if (detail.kind !== "turn_worker") return;

  const workId = detail.work_id?.trim() || detail.card.id;
  if (!isRelevantWorkerDetail(host, detail, workId)) return;

  const parentTurnId = detail.correlation_id?.trim() || null;
  const messageId = parentTurnId ? host.messageIdForTurn(parentTurnId) : null;
  const existing = host.workers.get(workId);
  const linkSessionId =
    existing?.sessionId ??
    (parentTurnId ? resolveTurnSessionId(host, parentTurnId) : host.sessionId);
  linkWorker(host, {
    workId,
    parentTurnId,
    messageId,
    sessionId: linkSessionId,
  });

  if (column === "wrapping_up" && previousColumn !== "wrapping_up") {
    noteWorkerSynthesizing(host, workId);
  }
  const isTerminal = column === "done" || (column === "blocked" && detail.terminal);
  const link = host.workers.get(workId);
  if (isTerminal && (previousColumn !== column || !link?.synthesisDelivered)) {
    void deliverWorkerSynthesis(host, workId, detail);
  }
}

export async function recoverPendingWorkerSyntheses(
  host: ChatStoreHost,
  cards: WorkCard[],
  details: Map<string, WorkCardDetail>,
) {
  for (const card of cards) {
    const detail = details.get(card.id);
    if (!detail || detail.kind !== "turn_worker") continue;
    const workId = detail.work_id?.trim() || card.id;
    if (!isRelevantWorkerDetail(host, detail, workId)) continue;
    const { workerTranscripts } = await import("$lib/work/workerTranscripts.svelte");
    workerTranscripts.ingestDetail(detail, card.column);
    onWorkerCardDetail(host, detail, card.column, undefined);
    const link = host.workers.get(workId);
    const isTerminal =
      card.column === "done" || (card.column === "blocked" && detail.terminal);
    if (link && !link.synthesisDelivered && isTerminal) {
      await deliverWorkerSynthesis(host, workId, detail);
    }
  }
}

export function pendingWorkerSynthesisIds(host: ChatStoreHost): Set<string> {
  const ids = new Set<string>();
  for (const [workId, link] of host.workers) {
    if (!link.synthesisDelivered) ids.add(workId);
  }
  return ids;
}

export function hasPendingWorkerSynthesis(host: ChatStoreHost, cardOrWorkId: string): boolean {
  const id = cardOrWorkId.trim();
  if (!id) return false;
  return pendingWorkerSynthesisIds(host).has(id);
}

export function noteWorkerSynthesisFailure(host: ChatStoreHost, workId: string, errorLine: string) {
  const link = host.workers.get(workId);
  if (!link || link.synthesisDelivered) return;
  const messageId = link.synthesisMessageId ?? link.messageId;
  if (!messageId) return;
  host.markMessageFailed(messageId, errorLine);
}

export function clearWorkerSynthesisFailure(host: ChatStoreHost, workId: string) {
  const link = host.workers.get(workId);
  if (!link) return;
  const messageId = link.synthesisMessageId ?? link.messageId;
  if (!messageId) return;

  const idx = host.messages.findIndex((message) => message.id === messageId);
  if (idx < 0 || !host.messages[idx].failed) return;

  const current = host.messages[idx];
  host.messages = [
    ...host.messages.slice(0, idx),
    {
      ...current,
      failed: false,
      errorLine: null,
      errorDetail: null,
      answerState: null,
      streaming: true,
      statusLine: "Loading result…",
    },
    ...host.messages.slice(idx + 1),
  ];
}

export async function retryWorkerSynthesis(host: ChatStoreHost, workId: string) {
  const trimmed = workId.trim();
  if (!trimmed) return;

  const link = host.workers.get(trimmed);
  if (!link || link.synthesisDelivered) return;

  clearWorkerSynthesisFailure(host, trimmed);

  const { workspace } = await import("$lib/stores/workspace.svelte");
  const detail = await workspace.fetchWorkerCardDetail(trimmed, true);
  const card = workspace.cards.find((item) => item.id === trimmed);
  if (!card || !detail || detail.kind !== "turn_worker") {
    noteWorkerSynthesisFailure(host, trimmed, "Couldn't load worker result. Tap to retry.");
    return;
  }

  onWorkerCardDetail(host, detail, card.column, undefined);
  await deliverWorkerSynthesis(host, trimmed, detail);
}

export function syncWorkerLaneFromCards(
  host: ChatStoreHost,
  cards: WorkCard[],
  details: Map<string, WorkCardDetail>,
) {
  for (const card of cards) {
    const detail = details.get(card.id);
    if (!detail || detail.kind !== "turn_worker") continue;
    const workId = detail.work_id?.trim() || card.id;
    if (!isRelevantWorkerDetail(host, detail, workId)) continue;
    const live = detail.live_status_line?.trim();
    const statusLine = live && live.length > 0 ? live : workerStatusLineForColumn(card.column);
    const streaming = card.column === "backlog" || card.column === "in_flight";
    updateWorkerLaneBubble(host, workId, { statusLine, streaming });
  }
}

function updateWorkerLaneBubble(
  host: ChatStoreHost,
  workId: string,
  options: { statusLine: string; streaming: boolean },
) {
  const link = host.workers.get(workId);
  const targetId = link?.synthesisMessageId;
  if (!targetId) return;
  const idx = host.messages.findIndex((message) => message.id === targetId);
  if (idx < 0) return;
  const current = host.messages[idx];
  host.messages = [
    ...host.messages.slice(0, idx),
    {
      ...current,
      lane: "worker",
      workId,
      statusLine: options.statusLine,
      streaming: options.streaming && !current.content.trim(),
    },
    ...host.messages.slice(idx + 1),
  ];
}

function noteWorkerSynthesizing(host: ChatStoreHost, workId: string) {
  const link = host.workers.get(workId);
  if (!link?.messageId) return;

  const idx = host.messages.findIndex((m) => m.id === link.messageId);
  if (idx < 0) return;

  const current = host.messages[idx];
  host.messages = [
    ...host.messages.slice(0, idx),
    {
      ...current,
      streaming: true,
      statusLine: "Pulling that together…",
    },
    ...host.messages.slice(idx + 1),
  ];
}

export function finalizeWorkerHandoffBubble(host: ChatStoreHost, messageId: string | null) {
  if (!messageId) return;
  const idx = host.messages.findIndex((m) => m.id === messageId);
  if (idx < 0) return;
  const current = host.messages[idx];
  host.messages = [
    ...host.messages.slice(0, idx),
    {
      ...current,
      streaming: false,
      phase: null,
      statusLine: null,
    },
    ...host.messages.slice(idx + 1),
  ];
}

export function ensureWorkerFollowUpBubble(
  host: ChatStoreHost,
  workId: string,
  turnId: string | null,
  options?: { statusLine?: string | null; streaming?: boolean },
): string {
  const link = host.workers.get(workId);

  if (link?.synthesisMessageId) {
    const existingIdx = host.messages.findIndex(
      (message) => message.id === link.synthesisMessageId,
    );
    if (existingIdx >= 0) {
      const current = host.messages[existingIdx];
      host.messages = [
        ...host.messages.slice(0, existingIdx),
        {
          ...current,
          streaming: options?.streaming ?? true,
          statusLine: options?.statusLine ?? current.statusLine,
        },
        ...host.messages.slice(existingIdx + 1),
      ];
      return link.synthesisMessageId;
    }
  }

  const id = randomUuid();
  host.messages = [
    ...host.messages,
    {
      id,
      role: "assistant",
      content: "",
      streaming: options?.streaming ?? true,
      turnId,
      lane: "worker",
      workId,
      statusLine: options?.statusLine ?? null,
    },
  ];

  if (link) {
    const nextWorkers = new Map(host.workers);
    nextWorkers.set(workId, { ...link, synthesisMessageId: id });
    host.workers = nextWorkers;
  }

  if (turnId) {
    const activeTurn = host.turns.get(turnId);
    if (activeTurn) {
      const nextTurns = new Map(host.turns);
      nextTurns.set(turnId, { ...activeTurn, messageId: id });
      host.turns = nextTurns;
    }
  }

  return id;
}

function hasFollowUpSynthesis(
  host: ChatStoreHost,
  handoffMessageId: string | null,
  content: string,
): boolean {
  if (!handoffMessageId) return false;
  const handoffIdx = host.messages.findIndex((m) => m.id === handoffMessageId);
  if (handoffIdx < 0) return false;
  const target = content.trim();
  return host.messages
    .slice(handoffIdx + 1)
    .some((message) => message.role === "assistant" && message.content.trim() === target);
}

async function resolveWorkerSynthesisContent(
  host: ChatStoreHost,
  link: WorkerLink,
  detail?: WorkCardDetail,
): Promise<string | null> {
  const handoffMessage = link.messageId
    ? host.messages.find((message) => message.id === link.messageId)
    : null;
  const handoffContent = handoffMessage?.stageWhisper?.trim() || handoffMessage?.content || null;

  const sessionIds = [link.sessionId];
  const workerSession = detail?.session_id?.trim();
  if (workerSession && !sessionIds.includes(workerSession)) {
    sessionIds.push(workerSession);
  }
  for (const sessionId of sessionIds) {
    const fromHistory = await fetchLatestAssistantTurn(sessionId, handoffContent);
    if (fromHistory) return fromHistory;
  }

  const excerpt = detail?.result_excerpt?.trim();
  if (excerpt) return excerpt;

  return detail?.error?.trim() || null;
}

export async function deliverWorkerSynthesis(
  host: ChatStoreHost,
  workId: string,
  detail?: WorkCardDetail,
) {
  const link = host.workers.get(workId);
  if (!link || link.synthesisDelivered) return;

  const content = await resolveWorkerSynthesisContent(host, link, detail);
  const isTerminal =
    detail?.card?.column === "done" ||
    (detail?.card?.column === "blocked" && detail.terminal === true);
  if (!content) {
    if (isTerminal) {
      noteWorkerSynthesisFailure(host, workId, "Worker finished, but the result didn't load.");
    }
    return;
  }

  if (hasFollowUpSynthesis(host, link.messageId, content)) {
    finalizeWorkerHandoffBubble(host, link.messageId);
    markWorkerSynthesisDelivered(host, workId);
    settleParentAfterWorkerSynthesis(host, link.parentTurnId);
    return;
  }

  const targetId =
    link.synthesisMessageId ??
    ensureWorkerFollowUpBubble(host, workId, link.parentTurnId, { streaming: false });
  if (targetId) {
    const idx = host.messages.findIndex((m) => m.id === targetId);
    if (idx >= 0) {
      host.messages = [
        ...host.messages.slice(0, idx),
        {
          ...host.messages[idx],
          content,
          streaming: false,
          failed: false,
          errorLine: null,
          answerState: null,
          phase: null,
          statusLine: null,
          lane: "worker",
          workId,
          tools: detail?.tool_names?.length ? [...detail.tool_names] : host.messages[idx].tools,
        },
        ...host.messages.slice(idx + 1),
      ];
      finalizeWorkerHandoffBubble(host, link.messageId);
      markWorkerSynthesisDelivered(host, workId);
      settleParentAfterWorkerSynthesis(host, link.parentTurnId);
      return;
    }
  }

  appendWorkerSynthesisMessage(host, workId, link.parentTurnId, content, detail?.tool_names);
  markWorkerSynthesisDelivered(host, workId);
  settleParentAfterWorkerSynthesis(host, link.parentTurnId);
}

function settleParentAfterWorkerSynthesis(host: ChatStoreHost, parentTurnId: string | null) {
  if (!parentTurnId) {
    host.noteBackgroundSettled();
    return;
  }
  const turn = host.turns.get(parentTurnId);
  if (!turn) {
    host.noteBackgroundSettled();
    return;
  }
  if (
    turn.mode === "background" ||
    turn.phase === "worker_handoff" ||
    turn.phase === "workshop_handoff"
  ) {
    host.settleTurn(parentTurnId);
    return;
  }
  host.noteBackgroundSettled();
}

export function markWorkerSynthesisDelivered(host: ChatStoreHost, workId: string) {
  const link = host.workers.get(workId);
  if (!link || link.synthesisDelivered) return;
  const nextWorkers = new Map(host.workers);
  nextWorkers.set(workId, { ...link, synthesisDelivered: true });
  host.workers = nextWorkers;
}

function appendWorkerSynthesisMessage(
  host: ChatStoreHost,
  workId: string,
  parentTurnId: string | null,
  content: string,
  toolNames?: string[] | null,
) {
  const link = host.workers.get(workId);
  const targetId = link?.messageId;
  if (targetId) {
    const idx = host.messages.findIndex((m) => m.id === targetId);
    if (idx >= 0) {
      host.messages = [
        ...host.messages.slice(0, idx),
        {
          ...host.messages[idx],
          content,
          streaming: false,
          phase: null,
          statusLine: null,
          tools: toolNames?.length ? [...toolNames] : host.messages[idx].tools,
        },
        ...host.messages.slice(idx + 1),
      ];
      if (link) {
        const nextWorkers = new Map(host.workers);
        nextWorkers.set(workId, { ...link, synthesisMessageId: targetId });
        host.workers = nextWorkers;
      }
      return;
    }
  }

  const id = randomUuid();
  host.messages = [
    ...host.messages,
    {
      id,
      role: "assistant",
      content,
      turnId: parentTurnId,
      tools: toolNames?.length ? [...toolNames] : undefined,
    },
  ];
  if (link) {
    const nextWorkers = new Map(host.workers);
    nextWorkers.set(workId, { ...link, synthesisMessageId: id });
    host.workers = nextWorkers;
  }
}

async function fetchLatestAssistantTurn(
  sessionId: string,
  skipContentMatching?: string | null,
): Promise<string | null> {
  try {
    const history = await getSessionHistory(sessionId, { limit: TRANSCRIPT_PAGE_SIZE });
    const assistants = [...history.turns].reverse().filter((turn) => turn.role === "assistant");
    const skip = skipContentMatching?.trim();
    if (skip) {
      const handoffTurn = assistants.find((turn) => turn.content.trim() === skip);
      if (handoffTurn) {
        const handoffIdx = history.turns.indexOf(handoffTurn);
        const after = history.turns
          .slice(handoffIdx + 1)
          .reverse()
          .find((turn) => turn.role === "assistant");
        return after?.content?.trim() || null;
      }
    }
    return assistants[0]?.content?.trim() || null;
  } catch {
    return null;
  }
}

export function handleWorkerSynthesisStreamEvent(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
) {
  const workId = event.work_id?.trim();
  const content = event.final_text?.trim();
  if (!workId || !content) return;

  if (!host.workers.has(workId)) {
    const handoffMessageId = host.messageIdForTurn(event.turn_id);
    if (handoffMessageId) {
      linkWorker(host, {
        workId,
        parentTurnId: event.turn_id,
        messageId: handoffMessageId,
        sessionId: resolveTurnSessionId(host, event.turn_id),
      });
      ensureWorkerFollowUpBubble(host, workId, event.turn_id, { streaming: false });
    }
  }

  const messageId = host.messageIdForTurn(event.turn_id);
  if (messageId) {
    host.applyStreamEventToMessage(messageId, event);
  } else {
    host.attachOrphanStream(event);
  }

  const link = host.workers.get(workId);
  if (link && !link.synthesisDelivered) {
    finalizeWorkerHandoffBubble(host, link.messageId);
    markWorkerSynthesisDelivered(host, workId);
  }

  host.syncTurnFromEvent(event);
  host.noteBackgroundSettled();
  if (host.shouldSettleTurnFromStream(event.turn_id)) {
    host.settleTurn(event.turn_id);
    host.scheduleSessionsRefresh();
  } else {
    void host.detachStreamOwner(event.turn_id);
  }
}
