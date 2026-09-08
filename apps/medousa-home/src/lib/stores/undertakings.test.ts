import { describe, expect, it } from "vitest";
import {
  findChatUndertakingContext,
  updateChatUndertakingBinding,
  type ActiveUndertakingContext,
} from "./undertakings.svelte";

function context(
  workId: string,
  boundChatSessionIds: string[] = [],
): ActiveUndertakingContext {
  return {
    workId,
    executionRuntimeId: null,
    executionTransportRuntimeId: null,
    repoId: null,
    title: workId,
    humanPhase: "work",
    forgeState: "ready",
    worktree: `/tmp/${workId}`,
    baselineOid: null,
    sealedOid: null,
    leaseId: null,
    leaseGeneration: null,
    executorKind: null,
    attemptSeq: null,
    boundChatSessionIds,
    boundTerminalSessionIds: [],
    selectedEntityId: null,
    selectedPath: null,
    selectedLine: null,
    selectionStartLine: null,
    selectionEndLine: null,
    selectedText: null,
  };
}

describe("undertaking chat bindings", () => {
  it("keeps a chat project visible while focus moves to an unbound pane", () => {
    const project = context("project-a", ["chat-a"]);
    const contexts = { codePane: project, chatPane: null };

    expect(findChatUndertakingContext(contexts, "chat-a", "chatPane")).toBe(project);
  });

  it("moves a chat binding between pane contexts instead of duplicating it", () => {
    const contexts = {
      codePane: context("project-a", ["chat-a"]),
      chatPane: context("project-a"),
    };

    const moved = updateChatUndertakingBinding(contexts, "chat-a", "chatPane");
    expect(moved.codePane?.boundChatSessionIds).toEqual([]);
    expect(moved.chatPane?.boundChatSessionIds).toEqual(["chat-a"]);
    expect(findChatUndertakingContext(moved, "chat-a", "chatPane")?.workId).toBe("project-a");

    const detached = updateChatUndertakingBinding(moved, "chat-a", null);
    expect(findChatUndertakingContext(detached, "chat-a", "chatPane")).toBeNull();
  });
});
