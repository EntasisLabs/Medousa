import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import { mergeTranscript } from "./mergeTranscript";
import { applyV3EnvelopeToMessage } from "$lib/stream/v3TranscriptReducer";
import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";

function saved(id: string, seq: number, role: ChatMessage["role"], content: string): ChatMessage {
  return { id, role, content, transcript: { authorityId: "authority", sessionId: "chat", entryId: id, entrySeq: seq } };
}

describe("mergeTranscript", () => {
  it("inserts a recovered earlier answer before the next user and active shell", () => {
    const first = saved("first", 1, "user", "First");
    const next = saved("next", 3, "user", "Next");
    const active: ChatMessage = { id: "live", role: "assistant", content: "Working", streaming: true, turnId: "active" };
    const result = mergeTranscript([first, next, active], [first, saved("answer", 2, "assistant", "Done"), next]);
    expect(result.map(message => message.id)).toEqual(["first", "answer", "next", "live"]);
    expect(result[3]).toBe(active);
  });
  it("repairs previously appended durable entries without separating the active prompt", () => {
    const first = saved("first", 1, "user", "First");
    const next = saved("next", 3, "user", "Next");
    const answer = saved("answer", 2, "assistant", "Done");
    const active: ChatMessage = { id: "live", role: "assistant", content: "Working", streaming: true };
    expect(mergeTranscript([first, next, active, answer], [first, answer, next]).map(message => message.id))
      .toEqual(["first", "answer", "next", "live"]);
  });
  it("places recovered history before a coordinate-less active prompt and shell", () => {
    const prompt: ChatMessage = { id: "prompt", role: "user", content: "Next" };
    const active: ChatMessage = { id: "live", role: "assistant", content: "", streaming: true };
    const recovered = saved("older", 2, "assistant", "Previous answer");
    expect(mergeTranscript([prompt, active], [recovered]).map(message => message.id)).toEqual(["older", "prompt", "live"]);
  });
  it("hydrates plain content on a settled empty shell without changing its render identity", () => {
    const result = mergeTranscript([{ id: "shell", role: "assistant", turnId: "turn", content: "", streaming: false }],
      [{ ...saved("entry", 2, "assistant", "Recovered answer"), turnId: "turn" }]);
    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ id: "shell", content: "Recovered answer", transcript: { entrySeq: 2 } });
  });
  it("preserves distinct durable prompts with identical text", () => {
    const first = saved("first", 1, "user", "continue");
    const second = saved("second", 3, "user", "continue");
    expect(mergeTranscript([first], [first, second]).map(message => message.id)).toEqual(["first", "second"]);
  });
  it("matches each optimistic prompt once and remains idempotent", () => {
    const local: ChatMessage[] = [{ id: "optimistic", role: "user", content: "continue" }];
    const history = [saved("first", 1, "user", "continue"), saved("second", 3, "user", "continue")];
    const result = mergeTranscript(local, history);
    expect(result.map(message => message.id)).toEqual(["optimistic", "second"]);
    expect(mergeTranscript(result, history)).toEqual(result);
  });
  it("never replaces a durable assistant with an equal-text entry from another turn", () => {
    const first = saved("first", 2, "assistant", "Done");
    const second = saved("second", 4, "assistant", "Done");
    expect(mergeTranscript([first], [second])).toEqual([first, second]);
  });
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
