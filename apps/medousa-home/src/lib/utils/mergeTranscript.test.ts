import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { mergeTranscript } from "./mergeTranscript";
import { applyV3EnvelopeToMessage } from "$lib/stream/v3TranscriptReducer";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";

describe("mergeTranscript", () => {
  it("keeps streamed segments at their consumed cursor when history is ahead", () => {
    const local: ChatMessage = {
      id: "live", role: "assistant", turnId: "turn", streaming: true,
      content: "Hello ", reasoning: "Think ",
      segments: [{ kind: "text", segmentId: "text", modelRound: 1, markdown: "Hello ", committed: false }],
    };
    const history: ChatMessage = {
      ...local, id: "saved", streaming: false, content: "Hello world", reasoning: "Think carefully",
      segments: [{ kind: "text", segmentId: "text", modelRound: 1, markdown: "Hello world", committed: true }],
    };
    let [message] = mergeTranscript([local], [history]);
    message = applyV3EnvelopeToMessage(message, {
      turn_id: "turn", seq: 2, event: { type: "content_append", segment_id: "text", text: "world" },
    } as TurnStreamEnvelopeV3);
    message = applyV3EnvelopeToMessage(message, {
      turn_id: "turn", seq: 3, event: { type: "reasoning_append", text: "carefully" },
    } as TurnStreamEnvelopeV3);
    expect(message.content).toBe("Hello world");
    expect(message.reasoning).toBe("Think carefully");
    expect(message.segments?.[0]).toMatchObject({ committed: false });
    const [settled] = mergeTranscript([{ ...message, streaming: false }], [history]);
    expect(settled.segments?.[0]).toMatchObject({ markdown: "Hello world", committed: true });
  });
  it("hydrates attachments by stable turn identity without replacing the streaming bubble", () => {
    const local = [{ id: "stream", role: "assistant", turnId: "turn", content: "Answer", streaming: true }] as ChatMessage[];
    const liveTranscripts = [{ id: "attachment", rows: [{ role: "user" as const, text: "Hi" }] }];
    const result = mergeTranscript(local, [{ id: "saved", role: "assistant", turnId: "turn", content: "Answer", liveTranscripts }] as ChatMessage[]);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ id: "stream", streaming: true, liveTranscripts });
  });
  it("hydrates generated media onto an already-rendered assistant bubble", () => {
    const local: ChatMessage[] = [{
      id: "live",
      role: "assistant",
      content: "Here is the image.",
      turnId: "turn-1",
      streaming: false,
    }];
    const daemon: ChatMessage[] = [{
      id: "session:entry",
      role: "assistant",
      content: "Here is the image.",
      mediaAttachments: [{
        mediaId: "gen:session:image",
        kind: "image",
        mime: "image/png",
        label: "Generated image",
        origin: "generated",
        generationId: "img:1",
      }],
    }];

    const [merged] = mergeTranscript(local, daemon);
    expect(merged.id).toBe("live");
    expect(merged.mediaAttachments?.[0]).toEqual(
      expect.objectContaining({ mediaId: "gen:session:image", generationId: "img:1" }),
    );
  });
});
