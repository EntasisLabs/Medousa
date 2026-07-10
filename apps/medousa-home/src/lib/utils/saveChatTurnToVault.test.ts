import { describe, expect, it } from "vitest";
import {
  assembleChatTurnNoteBody,
  assembleChatTurnNoteContent,
  canSaveAssistantTurn,
  chatTurnTitle,
} from "./saveChatTurnToVault";
import type { ChatMessage } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage> & Pick<ChatMessage, "id" | "role">): ChatMessage {
  return { content: "", ...partial };
}

describe("chatTurnTitle", () => {
  it("prefers the first markdown heading", () => {
    expect(chatTurnTitle("Intro\n\n# The Raven\n\nBody")).toBe("The Raven");
  });

  it("falls back to the first non-empty line", () => {
    expect(chatTurnTitle("Hello world\n\nMore")).toBe("Hello world");
  });

  it("skips fence openers", () => {
    expect(chatTurnTitle("```card\ntitle: X\n```\n\n# Real")).toBe("Real");
  });
});

describe("assembleChatTurnNoteBody", () => {
  it("includes a You preamble when a user prompt is present", () => {
    const body = assembleChatTurnNoteBody({
      title: "The Raven",
      assistantMarkdown: "Nevermore.",
      userPrompt: "Tell me about The Raven",
    });
    expect(body).toContain("# The Raven");
    expect(body).toContain("> **You**");
    expect(body).toContain("> Tell me about The Raven");
    expect(body).toContain("Nevermore.");
  });

  it("omits preamble when no user prompt", () => {
    const body = assembleChatTurnNoteBody({
      title: "Solo",
      assistantMarkdown: "Answer only",
    });
    expect(body).not.toContain("**You**");
    expect(body).toContain("Answer only");
  });
});

describe("assembleChatTurnNoteContent", () => {
  it("wraps inbox frontmatter with chat-turn tag and turn_index", () => {
    const content = assembleChatTurnNoteContent({
      title: "T",
      assistantMarkdown: "Body",
      turnIndex: 3,
    });
    expect(content.startsWith("---\n")).toBe(true);
    expect(content).toContain("kind: inbox");
    expect(content).toContain("tags: [chat-turn]");
    expect(content).toContain("turn_index: 3");
    expect(content).toContain("# T");
  });
});

describe("canSaveAssistantTurn", () => {
  it("allows settled non-empty assistant turns", () => {
    expect(
      canSaveAssistantTurn(msg({ id: "a1", role: "assistant", content: "Hello" })),
    ).toBe(true);
  });

  it("blocks streaming, empty, and user messages", () => {
    expect(
      canSaveAssistantTurn(
        msg({ id: "a1", role: "assistant", content: "Hi", streaming: true }),
      ),
    ).toBe(false);
    expect(canSaveAssistantTurn(msg({ id: "a1", role: "assistant", content: "  " }))).toBe(
      false,
    );
    expect(canSaveAssistantTurn(msg({ id: "u1", role: "user", content: "Hi" }))).toBe(false);
  });

  it("strips reasoning callouts before deciding emptiness", () => {
    const onlyReasoning = [
      "> [!abstract] Reasoning",
      "> plan the answer",
      "",
    ].join("\n");
    expect(
      canSaveAssistantTurn(msg({ id: "a1", role: "assistant", content: onlyReasoning })),
    ).toBe(false);

    const withBody = [
      "> [!abstract] Reasoning",
      "> plan",
      "",
      "Real answer",
    ].join("\n");
    expect(
      canSaveAssistantTurn(msg({ id: "a1", role: "assistant", content: withBody })),
    ).toBe(true);
  });
});
