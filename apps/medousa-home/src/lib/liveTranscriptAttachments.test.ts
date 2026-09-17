import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { attachLiveTranscripts } from "./liveTranscriptAttachments";

const rows = [{ role: "user", text: "Check GitHub" }, { role: "assistant", text: "It is available" }];
function attachment(target: string | null): ChatMessage {
  return { id: "chat:attachment", role: "assistant", content: target ? "" : "It is available", segments: [{ kind: "handoff", handoffKind: "live_transcript", workId: target, text: JSON.stringify(rows) }] } as ChatMessage;
}
describe("durable Live attachments", () => {
  it("attaches to the exact entry rather than the last answer", () => {
    const result = attachLiveTranscripts([
      { id: "chat:answer", role: "assistant", content: "Verified answer" } as ChatMessage,
      { id: "chat:later", role: "assistant", content: "Later answer" } as ChatMessage,
      attachment("answer"),
    ]);
    expect(result).toHaveLength(2);
    expect(result[0].liveTranscripts?.[0].rows).toEqual(rows);
    expect(result[1].liveTranscripts).toBeUndefined();
  });
  it("creates one expandable voice-only turn", () => {
    const result = attachLiveTranscripts([attachment(null)]);
    expect(result).toHaveLength(1);
    expect(result[0].content).toBe("It is available");
    expect(result[0].segments).toBeUndefined();
    expect(result[0].liveTranscripts?.[0].rows).toEqual(rows);
  });
  it("can link a transcript saved before a tool result was committed", () => {
    const result = attachLiveTranscripts([
      { id: "chat:answer", role: "assistant", turnId: "pending-turn", content: "Done" } as ChatMessage,
      attachment("execution:pending-turn"),
    ]);
    expect(result).toHaveLength(1);
    expect(result[0].liveTranscripts?.[0].rows).toEqual(rows);
  });
  it("keeps transcripts whose target is outside the loaded history page", () => {
    const result = attachLiveTranscripts([attachment("not-loaded")]);
    expect(result[0].content).toBe("Voice transcript");
    expect(result[0].liveTranscripts?.[0].rows).toEqual(rows);
  });
});
