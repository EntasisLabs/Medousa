/**
 * Paint ChatStoreHost fields from a ChatCore snapshot.
 * Core owns transcript/turn/worker identity; the host is a sync UI mirror.
 */

import type { ChatSnapshot } from "$lib/types/generated/app_api";
import type { ChatStoreHost } from "$lib/chat/chatStoreHost";
import {
  applySnapshotToMessages,
  applySnapshotTurns,
  applySnapshotWorkers,
} from "$lib/appChat/toChatMessage";

export type PaintFromSnapshotOptions = {
  /** Interactive turn whose live assistant bubble should become `assistantId`. */
  liveTurnId?: string | null;
  /** Prefer this assistant item id when present in the snapshot. */
  preferredAssistantId?: string | null;
};

/**
 * Replace host messages / turns / workers / seq from a core snapshot.
 * Does not clear alerts, narration, or stream ownership — callers handle those.
 */
export function paintHostFromSnapshot(
  host: ChatStoreHost,
  sessionId: string,
  snapshot: ChatSnapshot,
  options?: PaintFromSnapshotOptions,
) {
  const applied = applySnapshotToMessages(snapshot);
  host.messages = applied.messages;
  host.lastSeqByTurn = applied.lastSeqByTurn;
  host.workers = applySnapshotWorkers(host.workers, snapshot, sessionId);
  host.turns = applySnapshotTurns(snapshot, host.streamOwners);
  if (applied.activeTurnId !== undefined) {
    host.activeTurnId = applied.activeTurnId;
  }

  const liveTurnId = options?.liveTurnId?.trim();
  if (!liveTurnId) return;

  const turn = host.turns.get(liveTurnId);
  if (turn?.mode !== "interactive") return;

  const preferred = options?.preferredAssistantId?.trim();
  const assistant =
    (preferred
      ? applied.messages.find((message) => message.id === preferred)
      : undefined) ??
    applied.messages.find(
      (message) =>
        message.role === "assistant" && message.turnId === liveTurnId,
    );
  if (assistant) {
    host.assistantId = assistant.id;
  }
}
