import { describe, expect, it } from "vitest";

import { applyStreamEventToMessage } from "$lib/stream/transcriptReducer";
import type { ChatMessage } from "$lib/types/chat";

function assistant(content = ""): ChatMessage {
  return {
    id: "asst-1",
    role: "assistant",
    content,
    streaming: true,
    turnId: "turn-1",
  };
}

describe("applyStreamEventToMessage", () => {
  it("appends content without Svelte and returns next messages", () => {
    const result = applyStreamEventToMessage(
      [assistant()],
      0,
      {
        event_type: "content_delta",
        turn_id: "turn-1",
        seq: 1,
        content_delta: "Hello",
      } as never,
      { showEngineDetails: false },
    );
    expect(result.followUp).toBe("none");
    expect(result.messages[0]?.content).toBe("Hello");
  });

  it("replaces ACP assistant_message snapshots", () => {
    const result = applyStreamEventToMessage(
      [assistant("old")],
      0,
      {
        event_type: "assistant_message",
        turn_id: "turn-1",
        seq: 2,
        final_text: "new answer",
      } as never,
      { showEngineDetails: false },
    );
    expect(result.messages[0]?.content).toBe("new answer");
  });
});
