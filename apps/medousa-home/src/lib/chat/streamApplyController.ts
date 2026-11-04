/**
 * Live stream apply: core folds V3 facts; host applies snapshots then runs
 * side effects (alerts, narration, worker bookkeeping). Body authority is
 * medousa-app-core — TS transcript reducers remain for orphan/legacy helpers.
 */

import type {
  InteractiveTurnStreamEvent,
  PendingBudgetApproval,
} from "$lib/types/chat";
import { chatScenes } from "$lib/liquid/surfaces/chat/chatScenes.svelte";
import {
  operatorStreamErrorDetail,
  operatorStreamErrorLine,
  operatorStreamStatusLine,
  visibleChatStatusLine,
} from "$lib/utils/chatStreamDisplay";
import { shouldAcceptStreamEvent } from "$lib/utils/streamOwnership";
import { applyStreamSeq } from "$lib/stream/reconnect";
import type { StreamEventTarget } from "$lib/stream/eventPump";
import {
  applyStreamEventToMessage as reduceStreamEventToMessage,
  type StreamMessageFollowUp,
} from "$lib/stream/transcriptReducer";
import { v3EventPromotesChatMessage } from "$lib/stream/v3TranscriptReducer";
import { v3PresentationEvent } from "$lib/stream/v3PresentationAdapter";
import { stageWhisperAfterFinish } from "$lib/utils/turnInterimDisplay";
import { chatSettingsPort } from "$lib/runtime/chatSettingsPort";
import {
  isBudgetApprovalStreamEvent,
  isWorkerHandoffStreamEvent,
  isWorkerSynthesisStreamEvent,
  isWorkshopHandoffStreamEvent,
} from "$lib/utils/streamEvents";
import { budgetRequestIdFromStreamEvent, notifySpotlightTurnCompletion, registerSpotlightWorker } from "$lib/notifications";
import { randomUuid } from "$lib/utils/randomUuid";
import { handleWorkerSynthesisStreamEvent, workerLinkForTurn } from "$lib/chat/workerLaneController";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import { narration } from "$lib/stores/narration.svelte";
import { turnCompletionLedger } from "$lib/chat/turnCompletionLedger";
import {
  detachStreamOwner,
  finishMessage,
  markMessageFailed,
  recentlySettledTurnIdSet,
  shouldSettleTurnFromStream,
  settleTurn,
  transcriptTurnIdSet,
} from "$lib/chat/streamLifecycleController";
import {
  applyHistoryToSession,
  applyStreamEvent,
  openChatSession,
} from "$lib/appChat/client";
import { paintHostFromSnapshot } from "$lib/appChat/paintFromSnapshot";
import { chatMessageToItem } from "$lib/appChat/fromChatMessage";

const CONTENT_REVEAL_CHUNK_CHARS = 14;
const CONTENT_REVEAL_INTERVAL_MS = 16;

export function applyPumpedStreamEvent(host: ChatStoreHost, target: StreamEventTarget) {
  void applyPumpedStreamEventAsync(host, target);
}

async function applyPumpedStreamEventAsync(host: ChatStoreHost, target: StreamEventTarget) {
  const envelope = target.event;
  const event = envelope.event;
  const presentation = v3PresentationEvent(envelope);
  let messageId: string | null = null;
  let shouldProject = false;
  let accepted = false;

  host.withSessionFields(target.sessionId, () => {
    if (!isRelevantStreamEvent(host, presentation)) return;
    if (!applyStreamSeq(host.lastSeqByTurn, envelope)) return;
    accepted = true;

    if (
      event.type === "status" &&
      event.phase === "permission_resolved" &&
      host.permissionAlert?.turnId === envelope.turn_id
    ) {
      host.clearPermissionAlert();
    }
    if (event.type !== "secret_request" && host.secretAlert?.turnId === envelope.turn_id) {
      host.clearSecretAlert();
    }
    if (
      event.type !== "error" &&
      event.type !== "context_usage" &&
      event.type !== "worker_synthesis"
    ) {
      syncTurnFromEvent(host, presentation);
    }

    messageId =
      event.type === "tool_started" || event.type === "tool_finished"
        ? host.messageIdForToolStream(envelope.turn_id)
        : host.messageIdForTurn(envelope.turn_id);
    shouldProject = Boolean(messageId || v3EventPromotesChatMessage(event));
  });

  if (!accepted) return;

  if (shouldProject) {
    await applyStreamProjection(host, target.sessionId, envelope, messageId);
  }

  host.withSessionFields(target.sessionId, () => {
    if (!isRelevantStreamEvent(host, presentation)) return;
    messageId =
      messageId ??
      (event.type === "tool_started" || event.type === "tool_finished"
        ? host.messageIdForToolStream(envelope.turn_id)
        : host.messageIdForTurn(envelope.turn_id));

    switch (event.type) {
      case "context_usage":
        host.contextUsage = event.report;
        return;
      case "worker_synthesis":
        void notifySpotlightTurnCompletion(host.workshopScopeId ?? "", host.sessionId, envelope);
        handleWorkerSynthesisStreamEvent(host, presentation);
        return;
      case "browser_challenge":
        host.handleBrowserChallenge(presentation);
        return;
      case "permission_request":
        host.handlePermissionRequest(presentation);
        return;
      case "secret_request":
        host.handleSecretRequest(presentation);
        return;
      case "browser_navigated":
        host.handleBrowserNavigated(presentation);
        return;
      case "ui_scene":
        if (messageId) {
          chatScenes.applyWire(
            messageId,
            event.scene.surface_id?.trim() || `chat:${envelope.turn_id}`,
            event.scene.ops ?? [],
          );
        }
        return;
      case "worker_ack":
        registerSpotlightWorker(host.workshopScopeId ?? "", envelope);
        if (messageId) {
          releaseComposerHandoff(
            host,
            messageId,
            event.ack_kind === "workshop" ? "workshop_ack" : "worker_ack",
            presentation,
            { bodyFromCore: shouldProject },
          );
          host.scheduleSessionsRefresh();
        }
        return;
      case "budget_approval_required":
        if (messageId) {
          releaseComposerHandoff(host, messageId, "budget_approval", presentation, {
            bodyFromCore: shouldProject,
          });
        }
        return;
      case "turn_completed":
        void notifySpotlightTurnCompletion(host.workshopScopeId ?? "", host.sessionId, envelope);
        if (event.outcome === "failed" || event.outcome === "fuse_exhausted") {
          handleTurnError(host, presentation);
        } else if (messageId) {
          runMessageFollowUp(
            host,
            presentation,
            "terminal",
            messageId,
            undefined,
            event.outcome !== "cancelled",
            { bodyFromCore: shouldProject },
          );
        } else {
          noteTurnTerminal(host, presentation);
          if (!shouldProject) finishAskLaneTurn(host, envelope.turn_id);
        }
        return;
      default:
        return;
    }
  });
}

async function applyStreamProjection(
  host: ChatStoreHost,
  sessionId: string,
  envelope: StreamEventTarget["event"],
  knownMessageId: string | null,
) {
  const epoch = host.transcriptEpoch;
  const workshopEpoch = host.workshopEpoch;
  try {
    const authority =
      host.messages.find((message) => message.transcript?.authorityId)?.transcript
        ?.authorityId ?? `auth_${"0".repeat(64)}`;
    // BeginTurn dual-write normally seeds the shell. Reseed from host paint only
    // when core is missing that shell (race / reconnect), not on every event.
    const opened = await openChatSession(sessionId, authority);
    const hasTarget = knownMessageId
      ? opened.items.some((item) => item.id === knownMessageId)
      : opened.items.some(
          (item) =>
            item.role === "assistant" && item.turnId === envelope.turn_id,
        );
    if (!hasTarget && host.messages.length > 0) {
      await applyHistoryToSession({
        sessionId,
        history: {
          authority_id: authority,
          session_id: sessionId,
          turns: [],
        },
        replace: false,
        localItems: host.messages.map(chatMessageToItem),
      });
    }
    const snapshot = await applyStreamEvent(sessionId, envelope, {
      promoteItemId: knownMessageId ?? randomUuid(),
    });
    if (
      workshopEpoch !== host.workshopEpoch ||
      epoch !== host.transcriptEpoch ||
      host.history?.isReleased(sessionId)
    ) {
      return;
    }
    host.withSessionFields(sessionId, () => {
      paintHostFromSnapshot(host, sessionId, snapshot, {
        liveTurnId:
          envelope.event.type === "turn_completed" ? null : envelope.turn_id,
        preferredAssistantId: knownMessageId,
      });
    });
  } catch {
    // Stream projection failures fall through to terminal reconcile / reload.
  }
}

function replaceFinalTextFor(
  host: ChatStoreHost,
  messageId: string,
  event: InteractiveTurnStreamEvent,
): boolean {
  const workerLink = workerLinkForTurn(host.workers, event.turn_id);
  if (!workerLink) return false;
  const terminal = Boolean(event.final_text?.trim());
  const isWorkerSynthesisOnEnvelope =
    messageId === workerLink.messageId && terminal && Boolean(event.final_text?.trim());
  const isWorkerSynthesisTarget = messageId !== workerLink.messageId;
  return isWorkerSynthesisTarget || isWorkerSynthesisOnEnvelope;
}

export function applyStreamEventToMessage(
  host: ChatStoreHost,
  messageId: string,
  event: InteractiveTurnStreamEvent,
) {
  const idx = host.messageIndexForId(messageId);
  const result = reduceStreamEventToMessage(host.messages, idx, event, {
    showEngineDetails: chatSettingsPort().showEngineDetailsInChat(),
    replaceFinalText: replaceFinalTextFor(host, messageId, event),
  });
  if (result.followUp === "missing") {
    if (event.terminal) noteTurnTerminal(host, event);
    return;
  }
  if (idx >= 0 && result.messages[idx]) {
    host.replaceMessageAt(idx, result.messages[idx]);
  }
  runMessageFollowUp(host, event, result.followUp, messageId, result.revealContent);
}

function runMessageFollowUp(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
  followUp: StreamMessageFollowUp,
  messageId?: string,
  revealContent?: string,
  shouldNarrate = true,
  options?: { bodyFromCore?: boolean },
) {
  const id = messageId ?? host.messageIdForTurn(event.turn_id);
  const bodyFromCore = Boolean(options?.bodyFromCore);
  if (followUp === "missing") {
    if (event.terminal) noteTurnTerminal(host, event);
    return;
  }
  if (followUp === "worker_ack" && id) {
    releaseComposerHandoff(host, id, "worker_ack", event, { bodyFromCore });
    host.scheduleSessionsRefresh();
    return;
  }
  if (followUp === "workshop_ack" && id) {
    releaseComposerHandoff(host, id, "workshop_ack", event, { bodyFromCore });
    host.scheduleSessionsRefresh();
    return;
  }
  if (followUp === "budget_approval" && id) {
    releaseComposerHandoff(host, id, "budget_approval", event, { bodyFromCore });
    return;
  }
  if (followUp === "checkpoint_terminal" && id) {
    if (shouldNarrate) maybeNarrateTerminal(host, event, id);
    if (!bodyFromCore) {
      finishMessage(host, id);
      finishAskLaneTurn(host, event.turn_id);
    } else if (host.assistantId === id) {
      host.assistantId = null;
    }
    noteTurnTerminal(host, event);
    return;
  }
  if (followUp === "terminal" && id) {
    if (shouldNarrate) maybeNarrateTerminal(host, event, id, revealContent);
    if (bodyFromCore) {
      if (host.assistantId === id) host.assistantId = null;
      noteTurnTerminal(host, event, revealContent);
      return;
    }
    if (revealContent) {
      finishAskLaneTurn(host, event.turn_id);
      noteTurnTerminal(host, event, revealContent);
      revealContentText(host, id, revealContent);
      return;
    }
    finishMessage(host, id);
    finishAskLaneTurn(host, event.turn_id);
    noteTurnTerminal(host, event);
  }
}

function maybeNarrateTerminal(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
  messageId: string,
  canonicalContent?: string,
) {
  const turn = host.turns.get(event.turn_id);
  if (turn?.mode !== "interactive") return;
  const message = host.messages[host.messageIndexForId(messageId)];
  const text =
    canonicalContent?.trim() ||
    message?.content?.trim() ||
    event.final_text?.trim() ||
    "";
  if (!text) return;
  narration.maybeAutoNarrate(event.turn_id, messageId, text);
}

function handleTurnError(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const errorLine = operatorStreamErrorLine(
    event,
    chatSettingsPort().showEngineDetailsInChat(),
  );
  const errorDetail = operatorStreamErrorDetail(event, errorLine);
  host.streamError = errorLine;

  const messageId = host.messageIdForTurn(event.turn_id);
  if (messageId) {
    markMessageFailed(host, messageId, errorLine, errorDetail);
    if (host.assistantId === messageId) {
      host.assistantId = null;
    }
  }

  finishAskLaneTurn(host, event.turn_id);
  noteTurnTerminal(host, event);
  if (shouldSettleTurnFromStream(host, event.turn_id)) {
    settleTurn(host, event.turn_id);
  }
}

function finishAskLaneTurn(host: ChatStoreHost, turnId: string) {
  host.messages = host.messages.map((message) =>
    message.turnId === turnId && message.lane === "ask" && message.streaming
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
}

function noteTurnTerminal(host: ChatStoreHost, event: InteractiveTurnStreamEvent, canonicalContent?: string) {
  if (!shouldSettleTurnFromStream(host, event.turn_id)) return;
  const messageId = host.messageIdForTurn(event.turn_id);
  const message = host.messages.find((item) => item.role === "assistant"
    && (item.id === messageId || item.turnId === event.turn_id));
  const phase = event.event_type === "error" || event.phase === "error" ? "error"
    : event.phase === "cancelled" ? "cancelled" : "done";
  turnCompletionLedger.record(host.workshopScopeId ?? "", host.sessionId, event.turn_id, {
    terminal: true, phase, text: canonicalContent?.trim() || event.final_text?.trim() || message?.content || "",
  });
  settleTurn(host, event.turn_id);
  host.scheduleSessionsRefresh();
}

export function attachOrphanStream(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const workerLink = workerLinkForTurn(host.workers, event.turn_id);
  if (workerLink?.messageId) {
    applyStreamEventToMessage(host, workerLink.messageId, event);
    return;
  }

  const content = event.final_text ?? event.content_delta ?? "";
  if (!content && !event.terminal && event.event_type !== "budget_approval") {
    return;
  }

  const id = randomUuid();
  const turn = host.turns.get(event.turn_id);
  host.appendMessage({
    id,
    role: "assistant",
    content,
    streaming: !event.terminal,
    turnId: event.turn_id,
    phase: event.phase || null,
    statusLine: resolveStatusLine(event, null),
    tools: event.tool_names?.length ? [...event.tool_names] : undefined,
  });
  if (turn) {
    const next = new Map(host.turns);
    next.set(event.turn_id, { ...turn, messageId: id });
    host.turns = next;
  }
  if (workerLink && !workerLink.synthesisMessageId) {
    const nextWorkers = new Map(host.workers);
    nextWorkers.set(workerLink.workId, {
      ...workerLink,
      synthesisMessageId: id,
    });
    host.workers = nextWorkers;
  }
  if (turn?.mode === "interactive" && !event.terminal) {
    host.assistantId = id;
  }

  if (isWorkerHandoffStreamEvent(event)) {
    releaseComposerHandoff(host, id, "worker_ack", event);
    host.scheduleSessionsRefresh();
    return;
  }
  if (isWorkshopHandoffStreamEvent(event)) {
    releaseComposerHandoff(host, id, "workshop_ack", event);
    host.scheduleSessionsRefresh();
    return;
  }
  if (isBudgetApprovalStreamEvent(event)) {
    releaseComposerHandoff(host, id, "budget_approval", event);
    return;
  }
  if (event.terminal) {
    finishMessage(host, id);
    finishAskLaneTurn(host, event.turn_id);
    if (shouldSettleTurnFromStream(host, event.turn_id)) {
      settleTurn(host, event.turn_id);
      host.scheduleSessionsRefresh();
    }
  }
}

function releaseComposerHandoff(
  host: ChatStoreHost,
  messageId: string,
  phase: "worker_ack" | "workshop_ack" | "budget_approval",
  event: InteractiveTurnStreamEvent,
  options?: { bodyFromCore?: boolean },
) {
  const statusLine =
    event.message?.trim() ||
    (phase === "worker_ack"
      ? "Background worker started"
      : phase === "workshop_ack"
        ? "Medousa is in the workshop"
        : "Waiting for operator approval");

  const budgetRequestId =
    phase === "budget_approval" ? budgetRequestIdFromStreamEvent(event) : null;
  const requestedRounds = phase === "budget_approval" ? (event.requested_rounds ?? null) : null;
  const bodyFromCore = Boolean(options?.bodyFromCore);

  // When core already painted the handoff bubble, skip host body rewrites.
  if (!bodyFromCore) {
    const idx = host.messages.findIndex((m) => m.id === messageId);
    if (idx >= 0) {
      const current = host.messages[idx];
      const ackText = current.content.trim() || event.final_text?.trim() || statusLine;
      host.messages = [
        ...host.messages.slice(0, idx),
        {
          ...current,
          streaming: false,
          phase: phase === "budget_approval" ? "budget_blocked" : null,
          statusLine: phase === "budget_approval" ? statusLine : null,
          stageWhisper:
            phase === "worker_ack" || phase === "workshop_ack" ? ackText : current.stageWhisper,
          content: phase === "budget_approval" ? ackText : "",
          budgetRequestId,
          requestedRounds,
        },
        ...host.messages.slice(idx + 1),
      ];
    }

    const turn = host.turns.get(event.turn_id);
    if (turn) {
      const next = new Map(host.turns);
      next.set(event.turn_id, {
        ...turn,
        phase:
          phase === "worker_ack"
            ? "worker_handoff"
            : phase === "workshop_ack"
              ? "workshop_handoff"
              : "budget_blocked",
        messageId: phase === "budget_approval" ? messageId : null,
        workspaceCardId:
          phase === "budget_approval" && budgetRequestId ? budgetRequestId : turn.workspaceCardId,
        budgetRequestId,
        requestedRounds,
      });
      host.turns = next;
    }
  }

  if (host.assistantId === messageId) {
    host.assistantId = null;
  }
  if (host.activeTurnId === event.turn_id) {
    host.activeTurnId = null;
  }
  host.backgroundActivity += 1;

  if (phase === "worker_ack" || phase === "workshop_ack") {
    void detachStreamOwner(host, event.turn_id);
    host.linkWorkerFromStream(event, messageId);
    return;
  }

  if (budgetRequestId) {
    const alert: PendingBudgetApproval = {
      turnId: event.turn_id,
      messageId,
      requestId: budgetRequestId,
      workCardId: budgetRequestId,
      requestedRounds,
      message: statusLine,
    };
    host.budgetAlert = alert;
  }
}

function cancelContentReveal(host: ChatStoreHost, messageId: string) {
  const timer = host.contentRevealTimers.get(messageId);
  if (timer) {
    clearTimeout(timer);
    host.contentRevealTimers.delete(messageId);
  }
}

function patchMessageContent(host: ChatStoreHost, messageId: string, content: string) {
  const idx = host.messages.findIndex((message) => message.id === messageId);
  if (idx < 0) return;
  host.messages = [
    ...host.messages.slice(0, idx),
    { ...host.messages[idx], content },
    ...host.messages.slice(idx + 1),
  ];
}

function revealContentText(
  host: ChatStoreHost,
  messageId: string,
  fullText: string,
  onComplete?: () => void,
) {
  cancelContentReveal(host, messageId);
  let pos = 0;
  const step = () => {
    pos = Math.min(fullText.length, pos + CONTENT_REVEAL_CHUNK_CHARS);
    patchMessageContent(host, messageId, fullText.slice(0, pos));
    if (pos < fullText.length) {
      const timer = setTimeout(step, CONTENT_REVEAL_INTERVAL_MS);
      host.contentRevealTimers.set(messageId, timer);
      return;
    }
    host.contentRevealTimers.delete(messageId);
    onComplete?.();
  };
  step();
}

export function isRelevantStreamEvent(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
): boolean {
  const turnId = event.turn_id?.trim();
  if (!turnId) return false;

  if (
    isWorkerHandoffStreamEvent(event) ||
    isWorkshopHandoffStreamEvent(event) ||
    isWorkerSynthesisStreamEvent(event) ||
    isBudgetApprovalStreamEvent(event)
  ) {
    return true;
  }
  if (workerLinkForTurn(host.workers, turnId)) return true;

  const workId = event.work_id?.trim();
  if (workId && host.workers.has(workId)) return true;

  return shouldAcceptStreamEvent(turnId, host.streamOwners, host.turns, {
    recentlySettledTurnIds: recentlySettledTurnIdSet(host),
    transcriptTurnIds: transcriptTurnIdSet(host),
  });
}

export function syncTurnFromEvent(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  const existing = host.turns.get(event.turn_id);
  if (!existing) return;

  const workerLink = workerLinkForTurn(host.workers, event.turn_id);
  const preserveHandoff =
    workerLink != null &&
    !workerLink.synthesisDelivered &&
    !isWorkerHandoffStreamEvent(event) &&
    !isWorkshopHandoffStreamEvent(event) &&
    !isWorkerSynthesisStreamEvent(event) &&
    !isBudgetApprovalStreamEvent(event);
  const preservedPhase =
    existing.phase === "workshop_handoff" ? "workshop_handoff" : "worker_handoff";

  const next = new Map(host.turns);
  if (event.terminal) {
    if (existing.mode === "background") {
      next.set(event.turn_id, {
        ...existing,
        phase: preserveHandoff ? preservedPhase : phaseFromEvent(event),
        streamAttached: true,
        terminal: false,
      });
    } else if (shouldSettleTurnFromStream(host, event.turn_id)) {
      next.delete(event.turn_id);
    } else {
      next.set(event.turn_id, {
        ...existing,
        phase: preserveHandoff ? preservedPhase : phaseFromEvent(event),
        streamAttached: true,
        terminal: false,
      });
    }
  } else {
    next.set(event.turn_id, {
      ...existing,
      phase: preserveHandoff ? "worker_handoff" : phaseFromEvent(event),
      streamAttached: true,
      terminal: false,
    });
  }
  host.turns = next;
}

function resolveStatusLine(
  event: InteractiveTurnStreamEvent,
  current: string | null | undefined,
): string | null {
  if (event.message?.trim() || event.operator_message?.trim() || event.debug_message?.trim()) {
    return operatorStreamStatusLine(event, chatSettingsPort().showEngineDetailsInChat());
  }
  return visibleChatStatusLine(current, chatSettingsPort().showEngineDetailsInChat());
}

function phaseFromEvent(event: InteractiveTurnStreamEvent): string {
  if (isWorkerHandoffStreamEvent(event)) return "worker_handoff";
  if (isWorkshopHandoffStreamEvent(event)) return "workshop_handoff";
  if (isBudgetApprovalStreamEvent(event)) return "budget_blocked";
  if (event.terminal) return "done";
  return event.phase || "streaming";
}
