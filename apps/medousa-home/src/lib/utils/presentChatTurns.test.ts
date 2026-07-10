import { describe, expect, it } from "vitest";
import {
  isEmptyHandoffShell,
  presentChatMessages,
  presentWorkerThreadMessages,
} from "./presentChatTurns";
import type { ChatMessage } from "$lib/types/chat";

function msg(partial: Partial<ChatMessage>): ChatMessage {
  return { id: "m1", role: "assistant", content: "", ...partial } as ChatMessage;
}

describe("isEmptyHandoffShell", () => {
  it("detects whisper-only completed handoffs", () => {
    expect(
      isEmptyHandoffShell(
        msg({ stageWhisper: "Background worker started", streaming: false }),
      ),
    ).toBe(true);
  });

  it("keeps streaming or contentful turns", () => {
    expect(isEmptyHandoffShell(msg({ stageWhisper: "x", streaming: true }))).toBe(false);
    expect(isEmptyHandoffShell(msg({ content: "hi", stageWhisper: "x" }))).toBe(false);
  });

  it("keeps budget / error turns", () => {
    expect(isEmptyHandoffShell(msg({ stageWhisper: "x", budgetRequestId: "b1" }))).toBe(false);
    expect(isEmptyHandoffShell(msg({ stageWhisper: "x", failed: true, errorLine: "e" }))).toBe(
      false,
    );
  });
});

describe("presentChatMessages", () => {
  it("drops empty handoff shells from the list", () => {
    const user = msg({ id: "u", role: "user", content: "go" });
    const handoff = msg({ id: "h", stageWhisper: "Worker started" });
    const answer = msg({ id: "a", content: "done" });
    expect(presentChatMessages([user, handoff, answer]).map((m) => m.id)).toEqual([
      "u",
      "a",
    ]);
  });
});

describe("presentWorkerThreadMessages", () => {
  it("collapses handoff + synthesis to one row", () => {
    const handoff = msg({
      id: "h",
      lane: "worker",
      stageWhisper: "Background worker started",
    });
    const synth = msg({
      id: "s",
      lane: "worker",
      content: "Here is the result",
      workId: "w1",
    });
    const presented = presentWorkerThreadMessages([handoff, synth]);
    expect(presented).toHaveLength(1);
    expect(presented[0].id).toBe("s");
    expect(presented[0].content).toBe("Here is the result");
  });

  it("folds handoff whisper into an empty streaming synthesis pulse", () => {
    const handoff = msg({
      id: "h",
      lane: "worker",
      stageWhisper: "Medousa is in the workshop",
    });
    const synth = msg({
      id: "s",
      lane: "worker",
      content: "",
      streaming: true,
    });
    const presented = presentWorkerThreadMessages([handoff, synth]);
    expect(presented).toHaveLength(1);
    expect(presented[0].id).toBe("s");
    expect(presented[0].statusLine).toBe("Medousa is in the workshop");
  });
});
