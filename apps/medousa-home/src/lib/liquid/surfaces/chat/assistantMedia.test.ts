import { describe, expect, it } from "vitest";
import { findNode } from "$lib/liquid/core";
import type { ChatMessage } from "$lib/types/chat";
import { chatMessageToScene } from "./messageToScene";
import { resolveChatScene } from "./mergeBuilderBody";
import { presentChatMessages, presentWorkerThreadMessages } from "$lib/utils/presentChatTurns";

const attachments = [{ mediaId: "generated", kind: "image" as const, mime: "image/png", label: "Concept", origin: "generated" as const }];

describe("assistant generated media", () => {
  it("does not hide an image-only reply as an empty handoff", () => {
    const image: ChatMessage = { id: "image", role: "assistant", content: "", stageWhisper: "Done", mediaAttachments: attachments };
    const old: ChatMessage = { id: "old", role: "assistant", content: "Starting" };
    expect(presentChatMessages([image])).toEqual([image]);
    expect(presentWorkerThreadMessages([old, image])).toEqual([image]);
  });
  for (const streaming of [true, false]) {
    for (const segments of [undefined, [], [{ kind: "text" as const, segmentId: "answer", modelRound: 1, markdown: "Concept", committed: true }]]) {
      it(`renders attachments exactly once with streaming=${streaming}, segments=${segments?.length ?? "legacy"}`, () => {
        const message: ChatMessage = { id: "reply", role: "assistant", content: "Concept", mediaAttachments: attachments, streaming, segments };
        const scene = chatMessageToScene(message);
        expect(findNode(scene, "reply:media")?.props.attachments).toEqual(attachments);
        expect(scene.slots?.flow?.filter(node => node.type === "chat_media")).toHaveLength(1);
        expect(resolveChatScene(message, {}, null)).toEqual(scene);
      });
    }
  }
});
