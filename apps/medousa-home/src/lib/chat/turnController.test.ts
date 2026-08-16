import { describe, expect, it } from "vitest";

import { beginTurnMessages, turnStateFromTicket } from "$lib/chat/turnController";
import type { TurnTicketResponse } from "$lib/types/session";

const ticket: TurnTicketResponse = {
  turn_id: "turn-1",
  session_id: "session-1",
  mode: "interactive",
  phase: "streaming",
  accepted_at_utc: "2026-08-16T00:00:00Z",
  stream_url: "http://example/stream",
  stream_ready: true,
  workspace_card_id: null,
};

describe("turnController", () => {
  it("builds the user/assistant pair without the chat store", () => {
    const messages = beginTurnMessages({
      userContent: "hello",
      ticket,
      userMessageId: "user-1",
      assistantId: "asst-1",
    });
    expect(messages).toHaveLength(2);
    expect(messages[0]).toMatchObject({
      id: "user-1",
      role: "user",
      content: "hello",
      turnId: "turn-1",
      lane: "chat",
    });
    expect(messages[1]).toMatchObject({
      id: "asst-1",
      role: "assistant",
      streaming: true,
      turnId: "turn-1",
    });
  });

  it("registers ticket state with the assistant bubble id", () => {
    expect(turnStateFromTicket(ticket, "asst-1")).toMatchObject({
      turnId: "turn-1",
      messageId: "asst-1",
      streamAttached: true,
      terminal: false,
    });
  });
});
