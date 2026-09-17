import { describe, expect, it } from "vitest";
import { LiveTimeline, liveDelegation, liveDelegationResult } from "./liveProtocol";

describe("GPT-Live protocol", () => {
  it("partitions attachments at the next work request while retaining acknowledgments", () => {
    const timeline = new LiveTimeline();
    timeline.accept({ type: "session.input_transcript.delta", delta: "Check GitHub", start_ms: 10, end_ms: 30 });
    timeline.accept({ type: "session.output_transcript.delta", delta: "Checking", start_ms: 40, end_ms: 50 });
    timeline.accept({ type: "session.input_transcript.delta", delta: "Thank you", start_ms: 60, end_ms: 70 });
    timeline.accept({ type: "session.output_transcript.delta", delta: "GitHub is available", start_ms: 80, end_ms: 100 });
    timeline.accept({ type: "session.input_transcript.delta", delta: "Search a repository", start_ms: 110, end_ms: 140 });
    expect(timeline.requestStart(100, 150)).toBe(110);
    expect(timeline.transcriptRange(0, 110).map((row) => row.text)).toEqual(["Check GitHub", "Checking", "Thank you", "GitHub is available"]);
    expect(timeline.transcriptRange(110, Infinity)[0].text).toBe("Search a repository");
  });
  it("preserves fragments exactly, deduplicates delivery, and handles late timestamps", () => {
    const timeline = new LiveTimeline();
    const event = { type: "session.input_transcript.delta", event_id: "b", delta: " tools?", start_ms: 500, end_ms: 800 };
    expect(timeline.accept(event)).toBe(true);
    expect(timeline.accept(event)).toBe(false);
    timeline.accept({ ...event, event_id: "a", delta: "MCP", start_ms: 100, end_ms: 400 });
    expect(timeline.snapshot()[0].text).toBe("MCP tools?");
    expect(timeline.snapshot(400)[0].text).toBe("MCP");
  });

  it("groups each overlapping speaker independently without losing acknowledgments", () => {
    const timeline = new LiveTimeline();
    timeline.accept({ type: "session.input_transcript.delta", delta: "Check", start_ms: 0, end_ms: 300 });
    timeline.accept({ type: "session.output_transcript.delta", delta: "Okay", start_ms: 200, end_ms: 400 });
    timeline.accept({ type: "session.input_transcript.delta", delta: " MCP", start_ms: 350, end_ms: 600 });
    expect(timeline.snapshot().map((row) => row.text)).toEqual(["Check MCP", "Okay"]);
    expect(timeline.accept({ type: "session.input_transcript.delta", delta: "bad", start_ms: NaN, end_ms: 600 })).toBe(false);
  });

  it("requires client delegation metadata; never expects a generated task argument", () => {
    expect(liveDelegation({ type: "session.delegation.created", offset_ms: 20, delegation: { id: "opaque-id", target: "client" } }))
      .toEqual({ id: "opaque-id", offsetMs: 20 });
    expect(liveDelegation({ type: "session.delegation.created", offset_ms: 20, delegation: { id: "x", target: "responses" } })).toBeNull();
  });

  it("uses new speech for follow-up work rather than replaying the prior request", () => {
    const timeline = new LiveTimeline();
    timeline.accept({ type: "session.input_transcript.delta", delta: "Check MCP.", start_ms: 0, end_ms: 100 });
    timeline.accept({ type: "session.input_transcript.delta", delta: " Use that one.", start_ms: 300, end_ms: 400 });
    expect(timeline.requestBetween(-1, 200)).toBe("Check MCP.");
    expect(timeline.requestBetween(200, 500)).toBe("Use that one.");
    expect(timeline.snapshot(500)[0].text).toBe("Check MCP. Use that one.");
  });

  it("returns verified results through commentary without Realtime response triggers", () => {
    const result = liveDelegationResult("opaque-id", { status: "pending", text: "Still working" });
    expect(result.type).toBe("session.commentary.append");
    expect(result.delegation_id).toBe("opaque-id");
    expect(result.content).toContain("pending");
    expect(result).not.toHaveProperty("response");
    const long = liveDelegationResult("id", { status: "completed", text: "😀".repeat(1000) });
    expect(new TextEncoder().encode(long.content).length).toBeLessThan(500);
    expect(long.content).toContain("Full result is in the chat");
  });
});
