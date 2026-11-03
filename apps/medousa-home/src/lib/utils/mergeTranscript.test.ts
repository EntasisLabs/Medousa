import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { dedupeMessagesById } from "./mergeTranscript";
import {
  applyHistoryToSession,
  applySnapshotToMessages,
  openChatSession,
} from "$lib/appChat/client";
import type { ChatMessage } from "$lib/types/chat";

const AUTH = `auth_${"a".repeat(64)}`;

describe("dedupeMessagesById", () => {
  it("keeps the first occurrence of each id", () => {
    const messages: ChatMessage[] = [
      { id: "a", role: "user", content: "1" },
      { id: "a", role: "user", content: "2" },
      { id: "b", role: "assistant", content: "3" },
    ];
    expect(dedupeMessagesById(messages).map((message) => message.content)).toEqual(["1", "3"]);
  });
});

describe("app chat adapter consumes Rust snapshots", () => {
  beforeAll(async () => {
    await openChatSession("ses_adapter", AUTH);
  });

  it("applies history through the Rust core and maps camelCase items", async () => {
    const snapshot = await applyHistoryToSession({
      sessionId: "ses_adapter",
      history: {
        authority_id: AUTH,
        session_id: "ses_adapter",
        turns: [
          {
            entry_id: `ent_${"b".repeat(32)}`,
            entry_seq: 1,
            content_digest: "d",
            role: "user",
            content: "Hello",
            timestamp: "2026-08-25T00:00:00Z",
            tool_names: [],
          },
          {
            entry_id: `ent_${"c".repeat(32)}`,
            entry_seq: 2,
            content_digest: "d",
            role: "assistant",
            content: "Hi there",
            timestamp: "2026-08-25T00:00:01Z",
            tool_names: [],
            caused_by: {
              authority_id: AUTH,
              session_id: "ses_adapter",
              execution_id: "turn-1",
            },
          },
        ],
      },
      replace: true,
    });
    const applied = applySnapshotToMessages(snapshot);
    expect(applied.messages).toHaveLength(2);
    expect(applied.messages[0]).toMatchObject({
      role: "user",
      content: "Hello",
      transcript: { entrySeq: 1 },
    });
    expect(applied.messages[1]).toMatchObject({
      role: "assistant",
      content: "Hi there",
      turnId: "turn-1",
    });
    expect(applied.revision).toBeGreaterThan(0);
  });

  it("rejects stale session updates via expected revision", async () => {
    await expect(
      applyHistoryToSession({
        sessionId: "ses_adapter",
        history: {
          authority_id: AUTH,
          session_id: "ses_adapter",
          turns: [],
        },
        expectedRevision: 0,
        replace: false,
      }),
    ).rejects.toThrow(/stale revision|Application chat|rejected/i);
  });
});
