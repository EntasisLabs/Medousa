import { describe, expect, it } from "vitest";
import { chatMessageToScene } from "./messageToScene";
import { collectNodeIds, findNode } from "$lib/liquid/core";
import type { ChatMessage, ToolRunState } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage>): ChatMessage {
  return { id: "m1", role: "assistant", content: "", ...partial } as ChatMessage;
}

describe("chatMessageToScene — assistant", () => {
  it("wraps content in a document flow with a prose body", () => {
    const scene = chatMessageToScene(msg({ content: "Hello **world**" }));
    expect(scene.type).toBe("document");
    expect(scene.id).toBe("m1:doc");
    const body = findNode(scene, "m1:body");
    expect(body?.type).toBe("prose");
    expect(body?.props.markdown).toBe("Hello **world**");
  });

  it("emits a thinking node when reasoning is present", () => {
    const scene = chatMessageToScene(msg({ reasoning: "step 1", streaming: true }));
    expect(findNode(scene, "m1:thinking")?.type).toBe("thinking");
  });

  it("shows a Thinking… pill while streaming with no content", () => {
    const scene = chatMessageToScene(msg({ streaming: true }));
    const pill = findNode(scene, "m1:thinking-pill");
    expect(pill?.type).toBe("status_pill");
    expect(pill?.props.state).toBe("loading");
  });

  it("renders a status pill from a resolved status line", () => {
    const scene = chatMessageToScene(msg({ content: "hi", streaming: true }), {
      statusLine: "Searching the web…",
      statusWarn: false,
    });
    const status = findNode(scene, "m1:status");
    expect(status?.props.label).toBe("Searching the web…");
    expect(status?.props.state).toBe("loading");
  });

  it("renders an error callout + retry button", () => {
    const scene = chatMessageToScene(
      msg({ failed: true, errorLine: "boom", workId: "w9", content: "partial" }),
    );
    const error = findNode(scene, "m1:error");
    expect(error?.type).toBe("callout");
    expect(error?.props.tone).toBe("error");
    const retry = findNode(scene, "m1:retry");
    expect(retry?.type).toBe("button");
    expect(retry?.props.action).toBe("retry_worker");
    expect(retry?.props.payload).toEqual({ workId: "w9" });
  });

  it("maps structured tool runs to tool_trace", () => {
    const runs: ToolRunState[] = [
      { runId: "r1", toolName: "web.search", status: "succeeded", round: 1 },
    ];
    const scene = chatMessageToScene(msg({ content: "done", toolRuns: runs, turnIndex: 3 }));
    const tools = findNode(scene, "m1:tools");
    expect(tools?.type).toBe("tool_trace");
    expect(tools?.props.turnIndex).toBe(3);
  });

  it("falls back to a metadata line for plain tool names", () => {
    const scene = chatMessageToScene(msg({ content: "done", tools: ["web.search", "vault.read"] }));
    const tools = findNode(scene, "m1:tools");
    expect(tools?.type).toBe("metadata");
    expect(Array.isArray(tools?.props.parts)).toBe(true);
  });

  it("maps ui artifacts to a presentation node", () => {
    const scene = chatMessageToScene(
      msg({
        content: "see this",
        uiArtifacts: [
          { artifactId: "a1", mime: "text/html", label: "Chart", presentation: "panel" },
        ],
      }),
    );
    expect(findNode(scene, "m1:artifacts")?.type).toBe("presentation");
  });
});

describe("chatMessageToScene — user / system", () => {
  it("renders user text as plain prose (never parsed as markdown)", () => {
    const scene = chatMessageToScene(msg({ role: "user", content: "# not a heading" }));
    const body = findNode(scene, "m1:body");
    expect(body?.props.markdown).toBe("# not a heading");
    expect(body?.props.plain).toBe(true);
  });

  it("includes a chat_media node when the message has attachments", () => {
    const message = {
      ...msg({ role: "user", content: "look" }),
      mediaAttachments: [{ mediaId: "x", mime: "image/png", label: "shot" }],
    } as ChatMessage;
    const scene = chatMessageToScene(message);
    expect(findNode(scene, "m1:media")?.type).toBe("chat_media");
  });
});

describe("chatMessageToScene — reconciliation identity", () => {
  it("produces stable ids across rebuilds of the same message", () => {
    const message = msg({ content: "streaming…", streaming: true, reasoning: "r" });
    const a = collectNodeIds(chatMessageToScene(message));
    const b = collectNodeIds(chatMessageToScene(message));
    expect(a).toEqual(b);
  });
});
