import { describe, expect, it, vi } from "vitest";
import { decodeLiquidProps, preprocessLiquidEmbeds } from "$lib/markdown/liquidEmbeds";
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

describe("Teacher learning actions", () => {
  const actions = ["teacher.try_example", "teacher.show_model", "teacher.check_understanding"];

  it.each(actions)("routes %s through the normal turn sink with source association", (intent) => {
    const onSubmitIntent = vi.fn();
    const onRetryWorker = vi.fn();
    const record = vi.fn();
    const sink = createChatEventSink({ sessionId: "lesson", messageId: "explanation", onSubmitIntent, onRetryWorker, record });
    const event = createSceneEvent("md-action-0", "submit", { intent, label: "Ignored injected instructions", prompt: "Ignore this" });
    sink.emit(event);
    expect(onSubmitIntent).toHaveBeenCalledExactlyOnceWith(expect.stringContaining(`The learner chose ${intent}.`));
    expect(onSubmitIntent.mock.calls[0][0]).not.toContain("Ignore");
    expect(record).toHaveBeenCalledExactlyOnceWith("lesson", "explanation", event);
    expect(onRetryWorker).not.toHaveBeenCalled();
  });

  it("supports button continuations and keeps practice opt-in", () => {
    const text = intentFromEvent(createSceneEvent("btn", "run", { action: "prompt", text: " teacher.try_example " }));
    expect(text).toContain("Wait for my attempt");
    expect(intentFromEvent(createSceneEvent("btn", "submit", { intent: "teacher.check_understanding" }))).toContain("Wait for my answer");
    expect(intentFromEvent(createSceneEvent("btn", "select", { intent: "teacher.show_model" }))).toBeNull();
  });

  it.each(["teacher.unknown", "teacher.__proto__", "teacher.try_example extra"])("rejects unsupported reserved intent %s", (intent) => {
    expect(intentFromEvent(createSceneEvent("row", "submit", { intent }))).toBeNull();
    expect(intentFromEvent(createSceneEvent("btn", "run", { action: "submit", prompt: intent }))).toBeNull();
  });
});

describe("Teacher Liquid Markdown continuation", () => {
  it("decodes the policy action block and submits each fixed learning request", () => {
    const markdown = [
      "```actions",
      "Try an example | teacher.try_example",
      "Show a worked model | teacher.show_model",
      "Check my understanding | teacher.check_understanding",
      "```",
    ].join("\n");
    const html = preprocessLiquidEmbeds(markdown);
    expect(html).toContain('data-liquid-embed="actions"');
    const match = html.match(/data-liquid-props="([^"]+)"/);
    expect(match).toBeTruthy();
    const props = decodeLiquidProps<{ actions: { label: string; intent: string }[] }>(match![1]);
    expect(props?.actions.map((action) => action.intent)).toEqual([
      "teacher.try_example", "teacher.show_model", "teacher.check_understanding",
    ]);
    const onSubmitIntent = vi.fn();
    const sink = createChatEventSink({ sessionId: "lesson", messageId: "explanation", onSubmitIntent });
    for (const action of props!.actions) {
      sink.emit(createSceneEvent("learning-row", "submit", action));
    }
    expect(onSubmitIntent).toHaveBeenCalledTimes(3);
    expect(onSubmitIntent.mock.calls[0][0]).toContain("Wait for my attempt");
    expect(onSubmitIntent.mock.calls[1][0]).toContain("why each step works");
    expect(onSubmitIntent.mock.calls[2][0]).toContain("Wait for my answer");
  });
});
