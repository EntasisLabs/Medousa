import { describe, expect, it, vi } from "vitest";
import type { ChatStoreHost } from "./chatStoreHost";
import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

vi.mock("$lib/chat/streamLifecycleController", () => ({
  shouldSettleTurnFromStream: () => true,
  settleTurn: vi.fn(), finishMessage: vi.fn(),
}));
vi.mock("$lib/stores/narration.svelte", () => ({ narration: { maybeAutoNarrate: vi.fn() } }));
vi.mock("$lib/runtime/chatSettingsPort", () => ({ chatSettingsPort: () => ({ showEngineDetailsInChat: () => false }) }));
vi.mock("$lib/stream/transcriptReducer", () => ({
  applyStreamEventToMessage: (messages: unknown[]) => ({ messages, followUp: "terminal" }),
}));

import { applyStreamEventToMessage } from "./streamApplyController";
import { turnCompletionLedger } from "./turnCompletionLedger";

describe("successful stream completion", () => {
  it("records the answer on the normal success cleanup path", () => {
    const host = {
      workshopScopeId: "completion-test", sessionId: "chat",
      messages: [{ id: "assistant", role: "assistant", turnId: "turn", content: "GitHub is available." }],
      turns: new Map(), workers: [],
      messageIndexForId: () => 0, messageIdForTurn: () => "assistant",
      replaceMessageAt: vi.fn(), scheduleSessionsRefresh: vi.fn(),
    } as unknown as ChatStoreHost;
    applyStreamEventToMessage(host, "assistant", {
      turn_id: "turn", event_type: "done", terminal: true, phase: "done",
    } as InteractiveTurnStreamEvent);
    expect(turnCompletionLedger.read("completion-test", "chat", "turn")).toEqual({
      terminal: true, phase: "done", text: "GitHub is available.",
    });
  });
});
