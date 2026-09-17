import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { mergeTranscript } from "./mergeTranscript";

describe("mergeTranscript", () => {
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
