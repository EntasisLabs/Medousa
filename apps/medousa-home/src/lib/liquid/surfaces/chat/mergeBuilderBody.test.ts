import { describe, expect, it } from "vitest";
import { createNode } from "$lib/liquid/core";
import {
  extractBuilderBodyChildren,
  isBuilderBodyScene,
  resolveChatScene,
} from "./mergeBuilderBody";
import type { ChatMessage } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage> = {}): ChatMessage {
  return { id: "m1", role: "assistant", content: "fallback", ...partial } as ChatMessage;
}

describe("isBuilderBodyScene", () => {
  it("detects build: scene ids", () => {
    const root = createNode({
      id: "build:abc",
      type: "document",
      slots: {
        flow: [
          createNode({
            id: "build:abc:body",
            type: "stack",
            slots: { children: [] },
            fillState: "ready",
          }),
        ],
      },
      fillState: "ready",
    });
    expect(isBuilderBodyScene(root)).toBe(true);
  });
});

describe("resolveChatScene", () => {
  it("merges builder children ahead of observability", () => {
    const card = createNode({
      id: "build:x:card:1",
      type: "card",
      props: { title: "Mythos" },
      fillState: "ready",
    });
    const daemon = createNode({
      id: "build:x",
      type: "document",
      slots: {
        flow: [
          createNode({
            id: "build:x:body",
            type: "stack",
            slots: { children: [card] },
            fillState: "ready",
          }),
        ],
      },
      fillState: "ready",
    });

    const scene = resolveChatScene(
      msg({ content: "ignored prose", reasoning: "r1", streaming: false }),
      {},
      daemon,
    );
    const flow = scene.slots?.flow ?? [];
    expect(flow.some((n) => n.id === "build:x:card:1")).toBe(true);
    expect(flow.some((n) => n.id === "m1:thinking")).toBe(true);
    expect(flow.findIndex((n) => n.id === "m1:thinking")).toBeLessThan(
      flow.findIndex((n) => n.id === "build:x:card:1"),
    );
    expect(flow.some((n) => n.id === "m1:body")).toBe(false);
  });

  it("keeps legacy freeform daemon roots intact", () => {
    const root = createNode({
      id: "freeform",
      type: "document",
      slots: {
        flow: [createNode({ id: "only", type: "prose", props: { markdown: "x" }, fillState: "ready" })],
      },
      fillState: "ready",
    });
    expect(extractBuilderBodyChildren(root).length).toBeGreaterThan(0);
    // freeform without build:/:body stack — not a builder body
    expect(isBuilderBodyScene(root)).toBe(false);
    expect(resolveChatScene(msg(), {}, root)).toBe(root);
  });
});
