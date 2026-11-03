/**
 * Thin renderer adapter for the Rust application chat core.
 *
 * Production path: Tauri host (`app_chat_dispatch`).
 * Tests may install an in-process backend; they must not reimplement merge logic.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  BeginTurnRequest,
  ChatCommand,
  ChatItem,
  ChatLane,
  ChatSnapshot,
  ChatUpdate,
} from "$lib/types/generated/app_api";
import type { SessionHistoryResponse } from "$lib/types/session";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
import { applySnapshotToMessages, chatItemToMessage, snapshotMessages } from "./toChatMessage";
import type { ChatMessage } from "$lib/types/chat";

export type AppChatBackend = (command: ChatCommand) => Promise<ChatUpdate>;

let testBackend: AppChatBackend | null = null;

/** Vitest harnesses may install a backend; production uses Tauri / Node dispatch. */
export function installAppChatTestBackend(backend: AppChatBackend | null) {
  testBackend = backend;
}

export async function dispatchAppChat(command: ChatCommand): Promise<ChatUpdate> {
  if (testBackend) return testBackend(command);
  // Tauri host in production; browserCore aliases app_chat_* to the Rust
  // `app-chat-dispatch` binary under Vitest so there is still one engine.
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

export async function disposeChatSession(sessionId: string): Promise<void> {
  await dispatchAppChat({ type: "dispose_session", session_id: sessionId });
}

export async function readChatSnapshot(sessionId: string): Promise<ChatSnapshot> {
  return expectSnapshot(
    await dispatchAppChat({ type: "snapshot", session_id: sessionId }),
  );
}

export { applySnapshotToMessages, snapshotMessages, chatItemToMessage };
export type { ChatItem, ChatSnapshot, ChatUpdate };
