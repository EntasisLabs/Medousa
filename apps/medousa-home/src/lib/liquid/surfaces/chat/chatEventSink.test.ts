import { describe, expect, it, vi } from "vitest";
import { createSceneEvent } from "$lib/liquid/core";
import { createChatEventSink, intentFromEvent } from "./chatEventSink";

describe("intentFromEvent", () => {
  it("reads intent from a submit event", () => {
    expect(intentFromEvent(createSceneEvent("a", "submit", { intent: "Compare them" }))).toBe("Compare them");
  });

  it("reads prompt/text/intent from a run submit|prompt action", () => {
    expect(intentFromEvent(createSceneEvent("a", "run", { action: "submit", prompt: "Do it" }))).toBe("Do it");
    expect(intentFromEvent(createSceneEvent("a", "run", { action: "prompt", text: "Ask" }))).toBe("Ask");
    expect(intentFromEvent(createSceneEvent("a", "run", { action: "submit", intent: "Fallback" }))).toBe("Fallback");
  });

  it("returns null for run actions that are not submit/prompt", () => {
    expect(intentFromEvent(createSceneEvent("a", "run", { action: "retry_worker", workId: "w1" }))).toBeNull();
    expect(intentFromEvent(createSceneEvent("a", "run", { action: "open" }))).toBeNull();
  });

  it("returns null for non-turn event types and empty/blank intents", () => {
    expect(intentFromEvent(createSceneEvent("a", "select", { value: "x" }))).toBeNull();
    expect(intentFromEvent(createSceneEvent("a", "expand", { id: "a" }))).toBeNull();
    expect(intentFromEvent(createSceneEvent("a", "submit", { intent: "   " }))).toBeNull();
    expect(intentFromEvent(createSceneEvent("a", "submit", {}))).toBeNull();
  });
});

describe("createChatEventSink", () => {
  it("routes a submit event to onSubmitIntent and records it", () => {
    const onSubmitIntent = vi.fn();
    const record = vi.fn();
    const sink = createChatEventSink({ sessionId: "s1", messageId: "m1", onSubmitIntent, record });
    const event = createSceneEvent("row", "submit", { intent: "Compare them" });
    sink.emit(event);
    expect(onSubmitIntent).toHaveBeenCalledExactlyOnceWith("Compare them");
    expect(record).toHaveBeenCalledExactlyOnceWith("s1", "m1", event);
  });

  it("routes run+retry_worker to onRetryWorker, not onSubmitIntent", () => {
    const onSubmitIntent = vi.fn();
    const onRetryWorker = vi.fn();
    const sink = createChatEventSink({ sessionId: "s1", messageId: "m1", onSubmitIntent, onRetryWorker });
    sink.emit(createSceneEvent("btn", "run", { action: "retry_worker", workId: "w9" }));
    expect(onRetryWorker).toHaveBeenCalledExactlyOnceWith("w9");
    expect(onSubmitIntent).not.toHaveBeenCalled();
  });

  it("records select/expand without spawning a turn", () => {
    const onSubmitIntent = vi.fn();
    const record = vi.fn();
    const sink = createChatEventSink({ sessionId: "s1", messageId: "m1", onSubmitIntent, record });
    sink.emit(createSceneEvent("chip", "select", { value: "Under $2k" }));
    sink.emit(createSceneEvent("card", "expand", { id: "card" }));
    expect(onSubmitIntent).not.toHaveBeenCalled();
    expect(record).toHaveBeenCalledTimes(2);
  });

  it("records every event, including turn-spawning ones", () => {
    const record = vi.fn();
    const sink = createChatEventSink({ sessionId: "s1", messageId: "m1", record });
    sink.emit(createSceneEvent("row", "submit", { intent: "Go" }));
    sink.emit(createSceneEvent("chip", "select", { value: "x" }));
    expect(record).toHaveBeenCalledTimes(2);
  });
});
