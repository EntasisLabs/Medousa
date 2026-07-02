import type { TurnTicketState } from "$lib/types/chat";
import type { TurnTicketPhase, TurnTicketRecord } from "$lib/types/session";

const TERMINAL_PHASES = new Set<TurnTicketPhase>(["done", "error", "cancelled"]);

export interface StreamOwner {
  turnId: string;
  sessionId: string;
  streamUrl: string;
}

export interface StreamReattachContext {
  principalSessionId: string;
  isRelevantSession: (sessionId: string | null | undefined) => boolean;
  isDetachedWorkerTurn: (record: TurnTicketRecord) => boolean;
  localTurn?: TurnTicketState;
  hasAssistantMessage: boolean;
  assistantStreaming: boolean;
}

export function isTerminalTurnPhase(phase: TurnTicketPhase | string): boolean {
  return TERMINAL_PHASES.has(phase as TurnTicketPhase);
}

/** Whether Home should open (or keep) an SSE listener for this daemon turn ticket. */
export function shouldReattachTurnRecord(
  record: TurnTicketRecord,
  ctx: StreamReattachContext,
): boolean {
  if (ctx.isDetachedWorkerTurn(record)) return false;
  if (isTerminalTurnPhase(record.phase)) return false;
  if (!ctx.isRelevantSession(record.session_id)) return false;

  if (record.session_id.trim() !== ctx.principalSessionId.trim()) {
    if (record.mode !== "background") return false;
  }

  if (record.mode === "interactive" && (record.phase === "worker_handoff" || record.phase === "workshop_handoff")) {
    return false;
  }

  if (ctx.localTurn?.terminal) return false;
  if (ctx.localTurn?.phase === "worker_handoff" || ctx.localTurn?.phase === "workshop_handoff") return false;

  if (!ctx.localTurn && ctx.hasAssistantMessage && !ctx.assistantStreaming) {
    return false;
  }

  return true;
}

export function shouldAcceptStreamEvent(
  turnId: string,
  owners: ReadonlyMap<string, StreamOwner>,
  turns: ReadonlyMap<string, TurnTicketState>,
): boolean {
  if (owners.has(turnId)) return true;
  if (turns.has(turnId)) {
    const turn = turns.get(turnId)!;
    return !turn.terminal;
  }
  return false;
}
