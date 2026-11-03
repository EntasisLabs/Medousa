/**
 * Thin renderer adapter for the Rust application chat core.
 *
 * Production path: Tauri host (`app_chat_dispatch`).
 * Tests may install an in-process backend; they must not reimplement merge logic.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  AgentPlacementGroup,
  AskThreadBatch,
  BeginTurnRequest,
  ChatCommand,
  ChatItem,
  ChatLane,
  ChatSnapshot,
  ChatUpdate,
  ProposalFact,
  WorkerPlacementFact,
  WorkerReturnFact,
} from "$lib/types/generated/app_api";
import type { SessionHistoryResponse } from "$lib/types/session";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import { applySnapshotToMessages, chatItemToMessage, snapshotMessages } from "./toChatMessage";
import type { ChatMessage } from "$lib/types/chat";

export type AppChatBackend = (command: ChatCommand) => Promise<ChatUpdate>;

const TEST_BACKEND_KEY = "__medousaAppChatTestBackend";

type GlobalBackendHost = typeof globalThis & {
  [TEST_BACKEND_KEY]?: AppChatBackend | null;
};

function readTestBackend(): AppChatBackend | null {
  return (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] ?? null;
}

/**
 * Vitest harnesses install a backend (see `vitest/setupAppChat.ts`).
 * Stored on `globalThis` so `vi.resetModules()` cannot drop the shared engine.
 * Production uses the Tauri host.
 */
export function installAppChatTestBackend(backend: AppChatBackend | null) {
  (globalThis as GlobalBackendHost)[TEST_BACKEND_KEY] = backend;
}

export async function dispatchAppChat(command: ChatCommand): Promise<ChatUpdate> {
  const testBackend = readTestBackend();
  if (testBackend) return testBackend(command);
  return invoke<ChatUpdate>("app_chat_dispatch", { command });
}

function expectSnapshot(update: ChatUpdate): ChatSnapshot {
  if (update.type !== "snapshot") {
    throw new Error(`expected chat snapshot, got ${update.type}`);
  }
  return update.snapshot;
}

export async function openChatSession(
  sessionId: string,
  authorityId: string,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "open_session",
      session_id: sessionId,
      authority_id: authorityId ?? "",
    }),
  );
}

export async function applyHistoryToSession(options: {
  sessionId: string;
  history: SessionHistoryResponse;
  lane?: ChatLane | null;
  askJobId?: string | null;
  expectedRevision?: number | null;
  replace?: boolean;
  localItems?: ChatItem[] | null;
}): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "apply_history",
      session_id: options.sessionId,
      // Fixtures / partial daemon stubs may omit authority; never send null/undefined.
      authority_id: options.history.authority_id ?? "",
      turns: options.history.turns as unknown as Record<string, unknown>[],
      lane: options.lane ?? null,
      ask_job_id: options.askJobId ?? null,
      expected_revision: options.expectedRevision ?? null,
      replace: options.replace ?? false,
      local_items: options.localItems ?? null,
    }),
  );
}

/** Project history without mutating session state (exchange panes, previews). */
export async function projectHistory(
  history: SessionHistoryResponse,
  options?: { lane?: ChatLane | null; askJobId?: string | null },
): Promise<ChatMessage[]> {
  const update = await dispatchAppChat({
    type: "project_history",
    session_id: history.session_id ?? "",
    authority_id: history.authority_id ?? "",
    turns: history.turns,
    lane: options?.lane ?? null,
    ask_job_id: options?.askJobId ?? null,
  });
  if (update.type !== "projected") {
    throw new Error(`expected projected history, got ${update.type}`);
  }
  return update.items.map(chatItemToMessage);
}

export async function beginChatTurn(
  sessionId: string,
  request: BeginTurnRequest,
  expectedRevision?: number | null,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "begin_turn",
      session_id: sessionId,
      request,
      expected_revision: expectedRevision ?? null,
    }),
  );
}

export async function applyStreamEvent(
  sessionId: string,
  envelope: TurnStreamEnvelopeV3,
  options?: { expectedRevision?: number | null; promoteItemId?: string | null },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "apply_stream_event",
      session_id: sessionId,
      envelope,
      expected_revision: options?.expectedRevision ?? null,
      promote_item_id: options?.promoteItemId ?? null,
    }),
  );
}

export async function settleChatTurn(
  sessionId: string,
  turnId: string,
  options?: { expectedRevision?: number | null; reusableStream?: boolean },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "settle_turn",
      session_id: sessionId,
      turn_id: turnId,
      expected_revision: options?.expectedRevision ?? null,
      reusable_stream: options?.reusableStream ?? false,
    }),
  );
}

export async function linkChatWorker(
  sessionId: string,
  workId: string,
  options?: {
    parentTurnId?: string | null;
    parentItemId?: string | null;
    replyToWorkId?: string | null;
    expectedRevision?: number | null;
  },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "link_worker",
      session_id: sessionId,
      work_id: workId,
      parent_turn_id: options?.parentTurnId ?? null,
      parent_item_id: options?.parentItemId ?? null,
      reply_to_work_id: options?.replyToWorkId ?? null,
      expected_revision: options?.expectedRevision ?? null,
    }),
  );
}

export async function ensureChatWorkerFollowUp(
  sessionId: string,
  workId: string,
  options?: {
    turnId?: string | null;
    itemId?: string | null;
    statusLine?: string | null;
    streaming?: boolean | null;
    expectedRevision?: number | null;
  },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "ensure_worker_follow_up",
      session_id: sessionId,
      work_id: workId,
      turn_id: options?.turnId ?? null,
      item_id: options?.itemId ?? null,
      status_line: options?.statusLine ?? null,
      streaming: options?.streaming ?? null,
      expected_revision: options?.expectedRevision ?? null,
    }),
  );
}

export async function finalizeChatWorkerHandoff(
  sessionId: string,
  itemId: string,
  expectedRevision?: number | null,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "finalize_worker_handoff",
      session_id: sessionId,
      item_id: itemId,
      expected_revision: expectedRevision ?? null,
    }),
  );
}

export async function markChatWorkerSynthesisDelivered(
  sessionId: string,
  workId: string,
  expectedRevision?: number | null,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "mark_worker_synthesis_delivered",
      session_id: sessionId,
      work_id: workId,
      expected_revision: expectedRevision ?? null,
    }),
  );
}

export async function deliverChatWorkerSynthesis(
  sessionId: string,
  workId: string,
  content: string,
  options?: { tools?: string[] | null; expectedRevision?: number | null },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "deliver_worker_synthesis",
      session_id: sessionId,
      work_id: workId,
      content,
      tools: options?.tools ?? null,
      expected_revision: options?.expectedRevision ?? null,
    }),
  );
}

export async function applyChatProposals(
  sessionId: string,
  proposals: ProposalFact[],
  options?: {
    visibleItemIds?: string[] | null;
    expectedRevision?: number | null;
  },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "apply_proposals",
      session_id: sessionId,
      proposals,
      visible_item_ids: options?.visibleItemIds ?? null,
      expected_revision: options?.expectedRevision ?? null,
    }),
  );
}

export async function applyChatVisibleItems(
  sessionId: string,
  visibleItemIds: string[],
  expectedRevision?: number | null,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "apply_visible_items",
      session_id: sessionId,
      visible_item_ids: visibleItemIds,
      expected_revision: expectedRevision ?? null,
    }),
  );
}

/** Pure placement projection — does not require an open session when items are provided. */
export async function projectAgentPlacement(options: {
  sessionId: string;
  authorityId: string;
  items?: ChatItem[] | null;
  proposals?: ProposalFact[];
  workers?: WorkerPlacementFact[];
  visibleItemIds?: string[];
}): Promise<AgentPlacementGroup[]> {
  const update = await dispatchAppChat({
    type: "project_agent_placement",
    session_id: options.sessionId,
    authority_id: options.authorityId,
    items: options.items ?? null,
    proposals: options.proposals ?? [],
    workers: options.workers ?? [],
    visible_item_ids: options.visibleItemIds ?? [],
  });
  if (update.type !== "agent_placement") {
    throw new Error(`expected agent_placement, got ${update.type}`);
  }
  return update.groups;
}

/** Append ask-lane history batches into the focused chat session. */
export async function hydrateAskThreadsInCore(
  sessionId: string,
  batches: AskThreadBatch[],
  options?: {
    promotedAskIds?: string[];
    expectedRevision?: number | null;
  },
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "hydrate_ask_threads",
      session_id: sessionId,
      batches,
      promoted_ask_ids: options?.promotedAskIds ?? [],
      expected_revision: options?.expectedRevision ?? null,
    }),
  );
}

export async function promoteAskToChatInCore(
  sessionId: string,
  askJobId: string,
  expectedRevision?: number | null,
): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({
      type: "promote_ask_to_chat",
      session_id: sessionId,
      ask_job_id: askJobId,
      expected_revision: expectedRevision ?? null,
    }),
  );
}

/** Pure painted-id projection — empty handoff shells and exact linked raw returns. */
export async function projectVisibleItems(options: {
  sessionId: string;
  items?: ChatItem[] | null;
  workerReturns?: WorkerReturnFact[];
}): Promise<string[]> {
  const update = await dispatchAppChat({
    type: "project_visible_items",
    session_id: options.sessionId,
    items: options.items ?? null,
    worker_returns: options.workerReturns ?? [],
  });
  if (update.type !== "visible_items") {
    throw new Error(`expected visible_items, got ${update.type}`);
  }
  return update.item_ids;
}

export async function disposeChatSession(sessionId: string): Promise<void> {
  await dispatchAppChat({ type: "dispose_session", session_id: sessionId });
}

export async function readChatSnapshot(sessionId: string): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({ type: "snapshot", session_id: sessionId }),
  );
}

export { applySnapshotToMessages, snapshotMessages, chatItemToMessage };
export type {
  AgentPlacementGroup,
  AskThreadBatch,
  ChatItem,
  ChatSnapshot,
  ChatUpdate,
  ProposalFact,
  WorkerPlacementFact,
  WorkerReturnFact,
};
