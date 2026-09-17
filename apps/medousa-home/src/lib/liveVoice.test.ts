import { describe, expect, it } from "vitest";
import { liveHistoryEvents, livePhaseForServerEvent, liveTranscriptForServerEvent } from "$lib/liveVoice";

describe("Medousa Live server events", () => {
  it("does not persist partial or empty transcripts", () => {
    expect(liveTranscriptForServerEvent({ type: "response.output_audio_transcript.delta", transcript: "partial" })).toBeNull();
    expect(liveTranscriptForServerEvent({ type: "conversation.item.input_audio_transcription.completed", item_id: "a", transcript: " " })).toBeNull();
  });

  it("keeps the same message identity on event replay", () => {
    const event = { type: "response.output_audio_transcript.done", item_id: "reply", transcript: "Hello" };
    expect(liveTranscriptForServerEvent(event)).toEqual(liveTranscriptForServerEvent({ ...event, event_id: "retry" }));
  });
  it("seeds text history without requesting duplicate inference", () => {
    const events = liveHistoryEvents([
      { role: "user", content: "Remember this" },
      { role: "assistant", content: "Got it" },
    ]);
    expect(events.map((event) => event.item.content[0].type)).toEqual(["input_text", "output_text"]);
    expect(events.every((event) => event.type === "conversation.item.create")).toBe(true);
  });
  it("maps speech and response events to user-visible phases", () => {
    expect(livePhaseForServerEvent("input_audio_buffer.speech_started")).toBe("listening");
    expect(livePhaseForServerEvent("response.started")).toBe("thinking");
    expect(livePhaseForServerEvent("response.output_audio.started")).toBe("speaking");
    expect(livePhaseForServerEvent("response.done")).toBe("listening");
  });

  it("ignores unrelated events", () => {
    expect(livePhaseForServerEvent("session.started")).toBeNull();
  });

  it("extracts completed user and assistant transcripts", () => {
    expect(
      liveTranscriptForServerEvent({
        type: "conversation.item.input_audio_transcription.completed",
        item_id: "input-1",
        transcript: " Hey Medousa ",
      }),
    ).toEqual({ id: "user-input-1", role: "user", text: "Hey Medousa" });
    expect(
      liveTranscriptForServerEvent({
        type: "response.output_audio_transcript.done",
        item_id: "reply-1",
        transcript: "What's good?",
      }),
    ).toEqual({ id: "assistant-reply-1", role: "assistant", text: "What's good?" });
  });
});
