import { describe, expect, it } from "vitest";

import { mapTurns } from "$lib/chat/sessionController";
import type { SessionHistoryResponse } from "$lib/types/session";

function turn(
  parts: NonNullable<SessionHistoryResponse["turns"][number]["parts"]>,
): SessionHistoryResponse["turns"][number] {
  return {
    role: "assistant",
    content: "compatibility answer",
    content_digest: "digest",
    entry_id: "entry-1",
    entry_seq: 1,
    timestamp: "2026-08-25T00:00:00Z",
    tool_names: ["vault.read"],
    parts,
  };
}

describe("mapTurns chronological history", () => {
  it("uses durable entry coordinates for stable ids and turn indexes", () => {
    const messages = mapTurns([turn([])], { sessionId: "session-a" });

    expect(messages[0]?.id).toBe("session-a:entry-1");
    expect(messages[0]?.turnIndex).toBe(1);
  });

  it("hydrates segment-aware assistant parts into the V3 scene model", () => {
    const messages = mapTurns([
      turn([
        {
          kind: "text",
          markdown: "Let me check.",
          segment_id: "segment-a",
          model_round: 1,
        },
        {
          kind: "tool_run",
          run_id: "run-1",
          tool_name: "vault.read",
          status: "succeeded",
          input_summary: "read note",
          started_at: "2026-08-25T00:00:01Z",
          tool_round: 1,
        },
        {
          kind: "progress",
          markdown: "I found the relevant entries.",
        },
        {
          kind: "text",
          markdown: "Found it.",
          segment_id: "segment-b",
          model_round: 2,
        },
      ]),
    ]);

    expect(messages[0]?.segments?.map((segment) => segment.kind)).toEqual([
      "text",
      "tool_group",
      "progress",
      "text",
    ]);
    expect(messages[0]?.statusLine).toBeNull();
  });

  it("keeps pre-V3 history on the legacy flat layout", () => {
    const messages = mapTurns([
      turn([
        { kind: "text", markdown: "Old answer" },
        {
          kind: "tool_run",
          run_id: "run-1",
          tool_name: "vault.read",
          status: "succeeded",
          input_summary: "read note",
          started_at: "2026-08-25T00:00:01Z",
        },
      ]),
    ]);

    expect(messages[0]?.segments).toBeUndefined();
    expect(messages[0]?.content).toBe("compatibility answer");
    expect(messages[0]?.toolRuns?.[0]?.runId).toBe("run-1");
  });
});
