import { describe, expect, it } from "vitest";
import { chatMessageToScene } from "./messageToScene";
import { collectNodeIds, findNode } from "$lib/liquid/core";
import type { ChatMessage, ToolRunState } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage>): ChatMessage {
  return { id: "m1", role: "assistant", content: "", ...partial } as ChatMessage;
}

describe("chatMessageToScene — assistant order (thinking → body → tools)", () => {
  it("wraps content in a document flow with a prose body", () => {
    const scene = chatMessageToScene(msg({ content: "Hello **world**" }));
    expect(scene.type).toBe("document");
    expect(scene.id).toBe("m1:doc");
    const body = findNode(scene, "m1:body");
    expect(body?.type).toBe("prose");
    expect(body?.props.markdown).toBe("Hello **world**");
  });

  it("puts thinking above body and tools below", () => {
    const runs: ToolRunState[] = [
      { runId: "r1", toolName: "web.search", status: "succeeded", round: 1 },
    ];
    const scene = chatMessageToScene(
      msg({ content: "answer", reasoning: "step 1", toolRuns: runs }),
    );
    const flow = scene.slots?.flow ?? [];
    const ids = flow.map((n) => n.id);
    expect(ids.indexOf("m1:thinking")).toBeLessThan(ids.indexOf("m1:body"));
    expect(ids.indexOf("m1:body")).toBeLessThan(ids.indexOf("m1:tools"));
    expect(findNode(scene, "m1:obs")).toBeNull();
  });

  it("emits thinking directly (not under an observability drawer)", () => {
    const scene = chatMessageToScene(msg({ content: "hi", reasoning: "step 1", streaming: true }));
    expect(findNode(scene, "m1:thinking")?.type).toBe("thinking");
    expect(findNode(scene, "m1:obs")).toBeNull();
    expect(findNode(scene, "m1:pulse")).toBeNull();
  });

  it("shows a quiet live pulse while streaming with no content/reasoning", () => {
    const scene = chatMessageToScene(msg({ streaming: true }));
    const pulse = findNode(scene, "m1:pulse");
    expect(pulse?.type).toBe("status_pill");
    expect(pulse?.props.label).toBe("Thinking…");
    expect(pulse?.props.quiet).toBe(true);
  });

  it("folds status line into the live pulse", () => {
    const scene = chatMessageToScene(msg({ content: "hi", streaming: true }), {
      statusLine: "Searching the web…",
      statusWarn: false,
    });
    const pulse = findNode(scene, "m1:pulse");
    expect(pulse?.props.label).toBe("Searching the web…");
    expect(pulse?.props.quiet).toBe(true);
  });

  it("renders an error callout + retry button", () => {
    const scene = chatMessageToScene(
      msg({ failed: true, errorLine: "boom", workId: "w9", content: "partial" }),
    );
    expect(findNode(scene, "m1:error")?.type).toBe("callout");
    expect(findNode(scene, "m1:retry")?.props.action).toBe("retry_worker");
  });

  it("maps structured tool runs to tool_trace at the bottom", () => {
    const runs: ToolRunState[] = [
      { runId: "r1", toolName: "web.search", status: "succeeded", round: 1 },
    ];
    const scene = chatMessageToScene(msg({ content: "done", toolRuns: runs, turnIndex: 3 }));
    const tools = findNode(scene, "m1:tools");
    expect(tools?.type).toBe("tool_trace");
    expect(tools?.props.turnIndex).toBe(3);
    expect(tools?.props.compact).toBe(true);
  });

  it("maps plain tool names to host-style tool_trace lineage", () => {
    const scene = chatMessageToScene(msg({ content: "done", tools: ["web.search", "vault.read"] }));
    const tools = findNode(scene, "m1:tools");
    expect(tools?.type).toBe("tool_trace");
    expect(tools?.props.compact).toBe(true);
    const runs = tools?.props.runs as ToolRunState[];
    expect(runs).toHaveLength(2);
    expect(runs[0]?.toolName).toBe("web.search");
    expect(runs[0]?.status).toBe("succeeded");
    expect(runs[1]?.toolName).toBe("vault.read");
  });

  it("prefers structured toolRuns over plain tool name fallback", () => {
    const runs: ToolRunState[] = [
      { runId: "r1", toolName: "web.search", status: "succeeded", round: 1 },
    ];
    const scene = chatMessageToScene(
      msg({ content: "done", tools: ["ignored.name"], toolRuns: runs }),
    );
    const tools = findNode(scene, "m1:tools");
    expect(tools?.type).toBe("tool_trace");
    expect(tools?.props.runs).toEqual(runs);
  });
  it("maps ui artifacts to a presentation node near the body", () => {
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
  it("renders user text as plain prose", () => {
    const scene = chatMessageToScene(msg({ role: "user", content: "# not a heading" }));
    expect(findNode(scene, "m1:body")?.props.plain).toBe(true);
  });

  it("includes a chat_media node when the message has attachments", () => {
    const message = {
      ...msg({ role: "user", content: "look" }),
      mediaAttachments: [{ mediaId: "x", mime: "image/png", label: "shot" }],
    } as ChatMessage;
    expect(findNode(chatMessageToScene(message), "m1:media")?.type).toBe("chat_media");
  });
});

describe("chatMessageToScene — reconciliation identity", () => {
  it("produces stable ids across rebuilds of the same message", () => {
    const message = msg({ content: "streaming…", streaming: true, reasoning: "r" });
    expect(collectNodeIds(chatMessageToScene(message))).toEqual(
      collectNodeIds(chatMessageToScene(message)),
    );
  });
});
