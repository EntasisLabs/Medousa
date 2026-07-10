import { describe, expect, it } from "vitest";
import { stripChatBodyChrome } from "./stripChatBodyChrome";
import { chatMessageToScene } from "./messageToScene";
import type { ChatMessage } from "$lib/types/chat";

describe("stripChatBodyChrome", () => {
  it("removes a leading abstract Reasoning callout", () => {
    const src = [
      "> [!abstract] Reasoning",
      "> User wants a markdown-rich answer about Poe.",
      "> Use card, carousel, and actions.",
      "",
      "# Edgar Allan Poe",
      "",
      "His Essential Works",
    ].join("\n");
    const { markdown, recoveredReasoning } = stripChatBodyChrome(src);
    expect(markdown).not.toContain("[!abstract]");
    expect(markdown).toContain("# Edgar Allan Poe");
    expect(recoveredReasoning).toContain("markdown-rich answer");
  });

  it("leaves other callouts alone", () => {
    const src = "> [!note] Tip\n> Keep this\n\nBody";
    const { markdown, recoveredReasoning } = stripChatBodyChrome(src);
    expect(markdown).toContain("[!note]");
    expect(recoveredReasoning).toBeNull();
  });
});

describe("chatMessageToScene — leaked reasoning", () => {
  it("strips abstract callouts from body and feeds thinking", () => {
    const message = {
      id: "m1",
      role: "assistant",
      content: [
        "> [!abstract] Reasoning",
        "> Plan the embeds.",
        "",
        "Hello Poe",
      ].join("\n"),
      reasoning: null,
    } as ChatMessage;

    const scene = chatMessageToScene(message);
    const flow = scene.slots?.flow ?? [];
    const thinking = flow.find((n) => n.id === "m1:thinking");
    const body = flow.find((n) => n.id === "m1:body");
    expect(thinking?.props.reasoning).toContain("Plan the embeds");
    expect(String(body?.props.markdown ?? "")).toBe("Hello Poe");
    expect(String(body?.props.markdown ?? "")).not.toContain("[!abstract]");
  });
});
