/** Ports so workspace does not import the chat store. */

import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";

export type WorkspaceChatPort = {
  noteAskTurnSettled: (cardId: string) => void;
  hasPendingBudgetApproval: (cardId: string) => boolean;
  noteBackgroundSettled: () => void;
  noteBudgetResolved: (cardId: string) => void;
  syncWorkerLaneFromCards: (
    cards: WorkCard[],
    details: Map<string, WorkCardDetail>,
  ) => void;
  pendingWorkerSynthesisIds: () => Set<string>;
  recoverPendingWorkerSyntheses: (
    cards: WorkCard[],
    details: Map<string, WorkCardDetail>,
  ) => Promise<void>;
  onWorkerCardDetail: (
    detail: WorkCardDetail,
    column: string,
    previousColumn: string | undefined,
  ) => void;
  hasPendingWorkerSynthesis: (cardOrWorkId: string) => boolean;
  noteWorkerSynthesisFailure: (workId: string, errorLine: string) => void;
};

const unbound: WorkspaceChatPort = {
  noteAskTurnSettled: () => {},
  hasPendingBudgetApproval: () => false,
  noteBackgroundSettled: () => {},
  noteBudgetResolved: () => {},
  syncWorkerLaneFromCards: () => {},
  pendingWorkerSynthesisIds: () => new Set(),
  recoverPendingWorkerSyntheses: async () => {},
  onWorkerCardDetail: () => {},
  hasPendingWorkerSynthesis: () => false,
  noteWorkerSynthesisFailure: () => {},
};

let port: WorkspaceChatPort | null = null;

export function setWorkspaceChatPort(next: WorkspaceChatPort | null): void {
  port = next;
}

export function workspaceChatPort(): WorkspaceChatPort {
  return port ?? unbound;
}
