import { describe, expect, it } from "vitest";
import {
  groupChatTurnBeats,
  shouldForceExpandUserWhisper,
  userWhisperHook,
} from "./chatTurnBeats";
import type { ChatMessage } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage> & Pick<ChatMessage, "id" | "role">): ChatMessage {
  return {
    content: "",
    ...partial,
  };
}

describe("userWhisperHook", () => {
  it("takes the first line and truncates", () => {
    expect(userWhisperHook("Hello world")).toBe("Hello world");
    expect(userWhisperHook("a".repeat(50)).endsWith("…")).toBe(true);
    expect(userWhisperHook("Line one\nLine two")).toBe("Line one");
  });
});

describe("groupChatTurnBeats", () => {
  it("pairs user then assistant", () => {
    const beats = groupChatTurnBeats([
      msg({ id: "u1", role: "user", content: "hi" }),
      msg({ id: "a1", role: "assistant", content: "hello" }),
      msg({ id: "u2", role: "user", content: "more" }),
    ]);
    expect(beats).toHaveLength(2);
    expect(beats[0]).toMatchObject({ kind: "pair", user: { id: "u1" }, assistant: { id: "a1" } });
    expect(beats[1]).toMatchObject({ kind: "single", message: { id: "u2" } });
  });

  it("leaves orphan assistants alone", () => {
    const beats = groupChatTurnBeats([msg({ id: "a1", role: "assistant", content: "solo" })]);
    expect(beats).toEqual([{ kind: "single", message: expect.objectContaining({ id: "a1" }) }]);
  });
});

describe("shouldForceExpandUserWhisper", () => {
  it("expands the latest user while assistant streams", () => {
    const messages = [
      msg({ id: "u1", role: "user", content: "old" }),
      msg({ id: "a1", role: "assistant", content: "done" }),
      msg({ id: "u2", role: "user", content: "new" }),
      msg({ id: "a2", role: "assistant", content: "…", streaming: true }),
    ];
    expect(shouldForceExpandUserWhisper(messages, "u2")).toBe(true);
    expect(shouldForceExpandUserWhisper(messages, "u1")).toBe(false);
  });

  it("expands a trailing user with no reply yet", () => {
    const messages = [msg({ id: "u1", role: "user", content: "waiting" })];
    expect(shouldForceExpandUserWhisper(messages, "u1")).toBe(true);
  });
});
