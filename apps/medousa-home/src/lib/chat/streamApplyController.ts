/**
 * Stream leftover apply: reducer remainder + orphan/handoff follow-ups.
 * Body mutations live in `$lib/stream/transcriptReducer`.
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
} from "$lib/utils/chatStreamDisplay";
import { shouldAcceptStreamEvent } from "$lib/utils/streamOwnership";
import { applyStreamSeq } from "$lib/stream/reconnect";
import type { StreamEventTarget } from "$lib/stream/eventPump";
import {
  applyFocusedStreamEvent,
  applyStreamEventToMessage as reduceStreamEventToMessage,
  type StreamMessageFollowUp,
} from "$lib/stream/transcriptReducer";
import {
  applyV3EnvelopeToMessage,
  v3EventPromotesChatMessage,
} from "$lib/stream/v3TranscriptReducer";
import { v3PresentationEvent } from "$lib/stream/v3PresentationAdapter";
import { stageWhisperAfterFinish } from "$lib/utils/turnInterimDisplay";
import { chatSettingsPort } from "$lib/runtime/chatSettingsPort";
import {
  isBudgetApprovalStreamEvent,
  isWorkerHandoffStreamEvent,
  isWorkerSynthesisStreamEvent,
  isWorkshopHandoffStreamEvent,
} from "$lib/utils/streamEvents";
import { budgetRequestIdFromStreamEvent } from "$lib/notifications";
import { randomUuid } from "$lib/utils/randomUuid";
import { handleWorkerSynthesisStreamEvent, workerLinkForTurn } from "$lib/chat/workerLaneController";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import { narration } from "$lib/stores/narration.svelte";
import {
  detachStreamOwner,
  finishMessage,
  markMessageFailed,
  recentlySettledTurnIdSet,
  shouldSettleTurnFromStream,
  settleTurn,
  transcriptTurnIdSet,
} from "$lib/chat/streamLifecycleController";

const CONTENT_REVEAL_CHUNK_CHARS = 14;
const CONTENT_REVEAL_INTERVAL_MS = 16;

export function applyPumpedStreamEvent(host: ChatStoreHost, target: StreamEventTarget) {
  host.withSessionFields(target.sessionId, () => {
    const envelope = target.event;
    const event = envelope.event;
    const presentation = v3PresentationEvent(envelope);
    if (!isRelevantStreamEvent(host, presentation)) return;
    if (!applyStreamSeq(host.lastSeqByTurn, envelope)) return;

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

    const messageId =
      event.type === "tool_started" || event.type === "tool_finished"
        ? host.messageIdForToolStream(envelope.turn_id)
        : host.messageIdForTurn(envelope.turn_id);
    if (messageId) {
      const index = host.messageIndexForId(messageId);
      if (index >= 0) {
        host.replaceMessageAt(
          index,
          applyV3EnvelopeToMessage(host.messages[index], envelope),
        );
      }
    } else if (v3EventPromotesChatMessage(event)) {
      attachOrphanV3(host, target);
    }

    switch (event.type) {
      case "context_usage":
        host.contextUsage = event.report;
        return;
      case "worker_synthesis":
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
        if (messageId) {
          releaseComposerHandoff(
            host,
            messageId,
            event.ack_kind === "workshop" ? "workshop_ack" : "worker_ack",
            presentation,
          );
          host.scheduleSessionsRefresh();
        }
        return;
      case "budget_approval_required":
        if (messageId) {
          releaseComposerHandoff(host, messageId, "budget_approval", presentation);
        }
        return;
      case "turn_completed":
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
          );
        } else {
          noteTurnTerminal(host, presentation);
          finishAskLaneTurn(host, envelope.turn_id);
        }
        return;
      default:
        return;
    }
  });
}

function attachOrphanV3(host: ChatStoreHost, target: StreamEventTarget) {
  const envelope = target.event;
  const turn = host.turns.get(envelope.turn_id);
  const background = turn?.mode === "background";
  const id = randomUuid();
  const seed = applyV3EnvelopeToMessage(
    {
      id,
      role: "assistant",
      content: "",
      segments: [],
      streaming: envelope.event.type !== "turn_completed",
      turnId: envelope.turn_id,
      lane: background ? "ask" : "chat",
      askJobId: background ? turn?.workspaceCardId ?? envelope.turn_id : null,
      phase: null,
      statusLine: null,
    },
    envelope,
  );
  host.appendMessage(seed);

  if (turn) {
    const next = new Map(host.turns);
    next.set(envelope.turn_id, { ...turn, messageId: id });
    host.turns = next;
  }
  if (turn?.mode === "interactive" && envelope.event.type !== "turn_completed") {
    host.assistantId = id;
  }
}

export function applyStreamEventOnFocusedFields(
  host: ChatStoreHost,
  event: InteractiveTurnStreamEvent,
) {
  if (!isRelevantStreamEvent(host, event)) return;
  if (!applyStreamSeq(host.lastSeqByTurn, event)) return;

  if (
    event.event_type === "status" &&
    event.phase === "permission_resolved" &&
    host.permissionAlert?.turnId === event.turn_id
  ) {
    host.clearPermissionAlert();
  }

  if (
    event.event_type !== "secret_request" &&
    host.secretAlert?.turnId === event.turn_id
  ) {
    host.clearSecretAlert();
  }

  if (
    event.event_type !== "error" &&
    !(event.event_type === "context_usage" && event.context_usage) &&
    !isWorkerSynthesisStreamEvent(event)
  ) {
    syncTurnFromEvent(host, event);
  }

  const workerLink = workerLinkForTurn(host.workers, event.turn_id);
  const focused = applyFocusedStreamEvent(host.messages, event, {
    messageIdForTurn: (turnId) => host.messageIdForTurn(turnId),
    messageIdForToolStream: (turnId) => host.messageIdForToolStream(turnId),
    messageIndexForId: (messageId) => host.messageIndexForId(messageId),
    showEngineDetails: chatSettingsPort().showEngineDetailsInChat(),
    workerSynthesisDelivered: workerLink?.synthesisDelivered,
    replaceFinalTextFor: (messageId) => replaceFinalTextFor(host, messageId, event),
  });

  if (focused.messages !== host.messages) {
    const idx = remainderMessageIndex(host, focused.remainder);
    if (idx >= 0 && focused.messages[idx]) {
      host.replaceMessageAt(idx, focused.messages[idx]);
    } else {
      host.messages = focused.messages;
    }
  }

  const remainder = focused.remainder;
  switch (remainder.type) {
    case "error":
      handleTurnError(host, event);
      return;
    case "context_usage":
      host.contextUsage = remainder.usage;
      return;
    case "worker_synthesis":
      handleWorkerSynthesisStreamEvent(host, event);
      return;
    case "browser_challenge":
      host.handleBrowserChallenge(event);
      return;
    case "permission_request":
      host.handlePermissionRequest(event);
      return;
    case "secret_request":
      host.handleSecretRequest(event);
      return;
    case "browser_navigated":
      host.handleBrowserNavigated(event);
      return;
    case "ui_scene":
      if (event.ui_scene) {
        chatScenes.applyWire(
          remainder.messageId,
          event.ui_scene.surface_id?.trim() || `chat:${event.turn_id}`,
          event.ui_scene.ops ?? [],
        );
      }
      return;
    case "orphan":
      attachOrphanStream(host, event);
      return;
    case "terminal_empty":
      noteTurnTerminal(host, event);
      finishAskLaneTurn(host, event.turn_id);
      return;
    case "settle_delivered":
      runMessageFollowUp(host, event, "terminal");
      settleTurn(host, event.turn_id);
      host.scheduleSessionsRefresh();
      return;
    case "message":
      runMessageFollowUp(host, event, remainder.followUp, remainder.messageId, remainder.revealContent);
      return;
    default:
      return;
  }
}

function remainderMessageIndex(
  host: ChatStoreHost,
  remainder: ReturnType<typeof applyFocusedStreamEvent>["remainder"],
): number {
  if (remainder.type === "message") return host.messageIndexForId(remainder.messageId);
  if (remainder.type === "settle_delivered" && remainder.messageId) {
    return host.messageIndexForId(remainder.messageId);
  }
  return -1;
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
) {
  const id = messageId ?? host.messageIdForTurn(event.turn_id);
  if (followUp === "missing") {
    if (event.terminal) noteTurnTerminal(host, event);
    return;
  }
  if (followUp === "worker_ack" && id) {
    releaseComposerHandoff(host, id, "worker_ack", event);
    host.scheduleSessionsRefresh();
    return;
  }
  if (followUp === "workshop_ack" && id) {
    releaseComposerHandoff(host, id, "workshop_ack", event);
    host.scheduleSessionsRefresh();
    return;
  }
  if (followUp === "budget_approval" && id) {
    releaseComposerHandoff(host, id, "budget_approval", event);
    return;
  }
  if (followUp === "checkpoint_terminal" && id) {
    if (shouldNarrate) maybeNarrateTerminal(host, event, id);
    finishMessage(host, id);
    finishAskLaneTurn(host, event.turn_id);
    if (shouldSettleTurnFromStream(host, event.turn_id)) {
      settleTurn(host, event.turn_id);
      host.scheduleSessionsRefresh();
    }
    return;
  }
  if (followUp === "terminal" && id) {
    if (shouldNarrate) maybeNarrateTerminal(host, event, id, revealContent);
    if (revealContent) {
      finishAskLaneTurn(host, event.turn_id);
      if (shouldSettleTurnFromStream(host, event.turn_id)) {
        settleTurn(host, event.turn_id);
        host.scheduleSessionsRefresh();
      }
      revealContentText(host, id, revealContent);
      return;
    }
    finishMessage(host, id);
    finishAskLaneTurn(host, event.turn_id);
    if (shouldSettleTurnFromStream(host, event.turn_id)) {
      settleTurn(host, event.turn_id);
      host.scheduleSessionsRefresh();
    }
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

function noteTurnTerminal(host: ChatStoreHost, event: InteractiveTurnStreamEvent) {
  if (!shouldSettleTurnFromStream(host, event.turn_id)) return;
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
  if (event.message?.trim()) {
    return operatorStreamStatusLine(event, chatSettingsPort().showEngineDetailsInChat());
  }
  return current ?? null;
}

function phaseFromEvent(event: InteractiveTurnStreamEvent): string {
  if (isWorkerHandoffStreamEvent(event)) return "worker_handoff";
  if (isWorkshopHandoffStreamEvent(event)) return "workshop_handoff";
  if (isBudgetApprovalStreamEvent(event)) return "budget_blocked";
  if (event.terminal) return "done";
  return event.phase || "streaming";
}
