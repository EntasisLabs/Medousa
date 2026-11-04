import { describe, expect, it } from "vitest";
import type { ActiveUndertakingContext } from "$lib/stores/undertakings.svelte";
import {
  contextFromFact,
  paintUndertakingContextsFromSnapshot,
  toUndertakingContextFact,
} from "./paintFromSnapshot";

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
    selectedEntityId: "ent-1",
    selectedPath: "src/main.rs",
    selectedLine: 12,
    selectionStartLine: 10,
    selectionEndLine: 14,
    selectedText: "fn main",
  };
}

describe("undertaking binding paint", () => {
  it("round-trips binding fields through the core fact", () => {
    const host = context("project-a", ["chat-a"]);
    const fact = toUndertakingContextFact(host);
    expect(fact.workId).toBe("project-a");
    expect(fact.boundChatSessionIds).toEqual(["chat-a"]);
    const painted = contextFromFact(fact, host);
    expect(painted.boundChatSessionIds).toEqual(["chat-a"]);
    expect(painted.selectedPath).toBe("src/main.rs");
    expect(painted.selectedText).toBe("fn main");
  });

  it("preserves selection chrome when painting a snapshot", () => {
    const previous = {
      codePane: context("project-a", ["chat-a"]),
      chatPane: null,
    };
    const painted = paintUndertakingContextsFromSnapshot(
      {
        workshopScopeId: "personal",
        revision: 2,
        contexts: [
          {
            groupId: "codePane",
            context: {
              ...toUndertakingContextFact(previous.codePane!),
              boundChatSessionIds: [],
              title: "Project A",
            },
          },
          {
            groupId: "chatPane",
            context: toUndertakingContextFact(context("project-a", ["chat-a"])),
          },
        ],
      },
      previous,
    );
    expect(painted.codePane?.boundChatSessionIds).toEqual([]);
    expect(painted.codePane?.selectedPath).toBe("src/main.rs");
    expect(painted.chatPane?.boundChatSessionIds).toEqual(["chat-a"]);
    expect(painted.chatPane?.selectedPath).toBeNull();
  });
});
