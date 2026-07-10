import { beforeEach, describe, expect, it } from "vitest";
import { createSceneEvent } from "$lib/liquid/core";
import { chatInteractions } from "./chatInteractions";

beforeEach(() => {
  chatInteractions.reset();
});

describe("chatInteractions", () => {
  it("records and peeks events per session", () => {
    chatInteractions.record("s1", "m1", createSceneEvent("a", "select", { value: "x" }));
    chatInteractions.record("s1", "m1", createSceneEvent("b", "expand", { id: "b" }));
    const peeked = chatInteractions.peek("s1");
    expect(peeked).toHaveLength(2);
    expect(peeked[0].event.nodeId).toBe("a");
    expect(peeked[1].messageId).toBe("m1");
  });

  it("isolates sessions", () => {
    chatInteractions.record("s1", "m1", createSceneEvent("a", "select"));
    chatInteractions.record("s2", "m2", createSceneEvent("b", "select"));
    expect(chatInteractions.peek("s1")).toHaveLength(1);
    expect(chatInteractions.peek("s2")).toHaveLength(1);
    expect(chatInteractions.peek("s1")[0].event.nodeId).toBe("a");
  });

  it("drain returns and clears the session buffer", () => {
    chatInteractions.record("s1", "m1", createSceneEvent("a", "select"));
    chatInteractions.record("s1", "m1", createSceneEvent("b", "select"));
    const drained = chatInteractions.drain("s1");
    expect(drained).toHaveLength(2);
    expect(chatInteractions.peek("s1")).toHaveLength(0);
    expect(chatInteractions.drain("s1")).toHaveLength(0);
  });

  it("peek(n) returns the most recent n without mutating", () => {
    for (let i = 0; i < 5; i += 1) {
      chatInteractions.record("s1", "m1", createSceneEvent(`n${i}`, "select"));
    }
    const recent = chatInteractions.peek("s1", 2);
    expect(recent.map((e) => e.event.nodeId)).toEqual(["n3", "n4"]);
    expect(chatInteractions.peek("s1")).toHaveLength(5);
  });

  it("caps a session at 50, evicting the oldest", () => {
    for (let i = 0; i < 60; i += 1) {
      chatInteractions.record("s1", "m1", createSceneEvent(`n${i}`, "select"));
    }
    const all = chatInteractions.peek("s1");
    expect(all).toHaveLength(50);
    expect(all[0].event.nodeId).toBe("n10");
    expect(all[49].event.nodeId).toBe("n59");
  });

  it("ignores records with an empty session id", () => {
    chatInteractions.record("", "m1", createSceneEvent("a", "select"));
    expect(chatInteractions.peek("")).toHaveLength(0);
  });
});
