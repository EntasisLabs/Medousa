import { describe, expect, it, vi } from "vitest";
vi.mock("$lib/daemon", () => ({ getSessionHistory: vi.fn() }));
vi.mock("$lib/chat/sessionController", () => ({ TRANSCRIPT_PAGE_SIZE: 100 }));
import type { WorkCardDetail } from "$lib/types/card";
import * as client from "$lib/appChat/client";
import { linkWorker, onWorkerCardDetail } from "./workerLaneController";
import type { ChatStoreHost } from "./chatStoreHost";

function host(): ChatStoreHost {
  return { workers: new Map(), turns: new Map() } as unknown as ChatStoreHost;
}

describe("worker spawn linkage", () => {
  it("keeps the original spawn after card refresh resolves the turn to synthesis", () => {
    const state = host();
    linkWorker(state, { workId: "work", parentTurnId: "parent", messageId: "spawn", sessionId: "chat" });
    linkWorker(state, { workId: "work", parentTurnId: "parent", messageId: "result", sessionId: "chat" });
    expect(state.workers.get("work")?.messageId).toBe("spawn");
    linkWorker(state, { workId: "work", parentTurnId: "later", messageId: "later-reply", sessionId: "chat" });
    expect(state.workers.get("work")?.parentTurnId).toBe("parent");
    expect(state.workers.get("work")?.messageId).toBe("spawn");
  });
  it("hydrates coordinates missing when a card first arrives", () => {
    const state = host();
    linkWorker(state, { workId: "work", parentTurnId: null, messageId: null, sessionId: "chat" });
    linkWorker(state, { workId: "work", parentTurnId: "parent", messageId: "spawn", sessionId: "chat" });
    expect(state.workers.get("work")).toMatchObject({ parentTurnId: "parent", messageId: "spawn" });
  });
});

it("recovers the attributed worker result only after delivery is acknowledged", async () => {
  const sessionId = "origin";
  const host = {
    sessionId,
    workers: new Map(),
    turns: new Map([["parent", { phase: "streaming" }]]),
    messages: [{ role: "assistant", id: "later", content: "Another reply" }],
    messageIdForTurn: () => "parent-message",
    withSessionFields: vi.fn(),
    settleTurn: vi.fn(),
  } as unknown as ChatStoreHost;
  const snapshot = { sessionId, authorityId: "auth", revision: 1, items: [] };
  vi.spyOn(client, "linkChatWorker").mockResolvedValue(snapshot);
  const deliver = vi.spyOn(client, "deliverChatWorkerSynthesis").mockResolvedValue(snapshot);
  const detail: WorkCardDetail = {
    card: { id: "work", column: "done", status_label: "completed", title: "Worker",
      created_at_utc: "", updated_at_utc: "" },
    kind: "turn_worker", terminal: true, work_id: "work", session_id: sessionId,
    correlation_id: "parent", wrapping_up_reasons: ["delivery_delayed"],
    associations: { vault_paths: [], artifact_ids: [], locus_node_ids: [] },
    result_excerpt: "Excerpt",
    worker_transcript: { work_id: "work", session_id: sessionId, parent_runtime_id: "host",
      execution_runtime_id: "host", task_prompt: "Review", result_text: "Complete result",
      steer_messages: [], retention_note: "" },
  };
  try {
    await onWorkerCardDetail(host, detail, "done", "in_flight");
    expect(deliver).not.toHaveBeenCalled();
    await onWorkerCardDetail(host, { ...detail, wrapping_up_reasons: [] }, "done", "done");
    expect(deliver).toHaveBeenCalledExactlyOnceWith(sessionId, "work", "Complete result", { tools: null });
    await onWorkerCardDetail(host, {
      ...detail,
      card: { ...detail.card, id: "cancelled-work", column: "blocked", status_label: "cancelled" },
      work_id: "cancelled-work", worker_transcript: null, result_excerpt: "", wrapping_up_reasons: [],
    }, "blocked", "in_flight");
    expect(deliver).toHaveBeenLastCalledWith(sessionId, "cancelled-work", "Worker cancelled.", { tools: null });
  } finally {
    vi.restoreAllMocks();
  }
});
