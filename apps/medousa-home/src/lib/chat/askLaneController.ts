/**
 * Ask-lane hydrate / promote into the focused chat session.
 * Daemon fetch stays here; merge/lane identity live in medousa-app-core.
 */

import { getSessionHistory } from "$lib/daemon";
import type { SessionHistoryResponse } from "$lib/types/session";
import type { WorkCard } from "$lib/types/workspace";
import { isAskJobId, askSessionId } from "$lib/types/askJob";
import type { AskThreadBatch } from "$lib/types/generated/app_api";
import {
  applySnapshotToMessages,
  hydrateAskThreadsInCore,
  openChatSession,
  promoteAskToChatInCore,
} from "$lib/appChat/client";
import { dedupeMessagesById } from "$lib/utils/mergeTranscript";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import {
  mapTurns,
  savePromotedAskIds,
  TRANSCRIPT_PAGE_SIZE,
} from "$lib/chat/sessionController";

export function promoteAskToChat(host: ChatStoreHost, jobId: string) {
  const trimmed = jobId.trim();
  if (!trimmed) return;
  host.promotedAskIds.add(trimmed);
  savePromotedAskIds(host.promotedAskIds, host.workshopScopeId);

  const sessionId = host.sessionId.trim();
  if (!sessionId) {
    host.messages = dedupeMessagesById(
      host.messages.map((message) =>
        message.askJobId === trimmed ? { ...message, lane: "chat", askJobId: null } : message,
      ),
    );
    return;
  }
  // Core owns lane identity; paint from the returned snapshot.
  void promoteAskToChatInCore(sessionId, trimmed)
    .then((snapshot) => {
      if (host.sessionId !== sessionId) return;
      const applied = applySnapshotToMessages(snapshot);
      host.messages = applied.messages;
    })
    .catch(() => {
      if (host.sessionId !== sessionId) return;
      host.messages = dedupeMessagesById(
        host.messages.map((message) =>
          message.askJobId === trimmed ? { ...message, lane: "chat", askJobId: null } : message,
        ),
      );
    });
}

export async function hydrateAskThreads(host: ChatStoreHost, cards: WorkCard[]) {
  const workshopEpoch = host.workshopEpoch;
  const epoch = host.transcriptEpoch;
  const focusedSessionId = host.sessionId.trim();
  const targets = cards.filter((card) => {
    if (!isAskJobId(card.id)) return false;
    if (host.promotedAskIds.has(card.id)) return false;
    if (host.askHydrationInFlight.has(card.id)) return false;
    return !host.messages.some((message) => message.askJobId === card.id);
  });
  if (targets.length === 0) return;

  for (const card of targets) {
    host.askHydrationInFlight.add(card.id);
  }

  try {
    const fetched = await Promise.all(
      targets.map(async (card) => {
        try {
          const askLedgerId = askSessionId(card.id);
          const history = await getSessionHistory(askLedgerId, {
            limit: TRANSCRIPT_PAGE_SIZE,
          });
          if (
            workshopEpoch !== host.workshopEpoch ||
            epoch !== host.transcriptEpoch ||
            history.turns.length === 0
          ) {
            return null;
          }
          const batch: AskThreadBatch = {
            ask_job_id: card.id,
            ask_session_id: askLedgerId,
            authority_id: history.authority_id ?? "",
            turns: history.turns as unknown[],
          };
          return batch;
        } catch {
          return null;
        }
      }),
    );

    if (workshopEpoch !== host.workshopEpoch || epoch !== host.transcriptEpoch) return;

    const batches = fetched.filter(
      (batch): batch is AskThreadBatch => batch !== null,
    );
    if (batches.length === 0) return;

    if (focusedSessionId && host.sessionId.trim() === focusedSessionId) {
      const authority =
        batches.find((batch) => batch.authority_id.trim())?.authority_id
        ?? host.messages.find((message) => message.transcript?.authorityId)?.transcript
          ?.authorityId
        ?? "";
      try {
        await openChatSession(focusedSessionId, authority);
        const snapshot = await hydrateAskThreadsInCore(focusedSessionId, batches, {
          promotedAskIds: [...host.promotedAskIds],
        });
        if (workshopEpoch !== host.workshopEpoch || epoch !== host.transcriptEpoch) return;
        if (host.sessionId !== focusedSessionId) return;
        const applied = applySnapshotToMessages(snapshot);
        host.messages = applied.messages;
        if (applied.activeTurnId) host.activeTurnId = applied.activeTurnId;
        for (const [turnId, seq] of applied.lastSeqByTurn) {
          host.lastSeqByTurn.set(turnId, seq);
        }
        return;
      } catch {
        // Fall through to host-only merge when the core path is unavailable.
      }
    }

    const projected = await Promise.all(
      batches.map((batch) =>
        mapTurns(batch.turns as SessionHistoryResponse["turns"], {
          lane: "ask",
          askJobId: batch.ask_job_id,
          sessionId: batch.ask_session_id,
          authorityId: batch.authority_id,
        }),
      ),
    );
    if (workshopEpoch !== host.workshopEpoch || epoch !== host.transcriptEpoch) return;

    const hydrated = projected.flat();
    if (hydrated.length === 0) return;

    const jobsAlreadyHydrated = new Set(
      host.messages
        .map((message) => message.askJobId)
        .filter((jobId): jobId is string => Boolean(jobId?.trim())),
    );
    const fresh = hydrated.filter((message) => {
      const jobId = message.askJobId?.trim();
      if (!jobId) return true;
      if (host.promotedAskIds.has(jobId)) return false;
      if (jobsAlreadyHydrated.has(jobId)) return false;
      return true;
    });
    if (fresh.length === 0) return;

    host.messages = dedupeMessagesById([...host.messages, ...fresh]);
  } finally {
    for (const card of targets) {
      host.askHydrationInFlight.delete(card.id);
    }
  }
}
