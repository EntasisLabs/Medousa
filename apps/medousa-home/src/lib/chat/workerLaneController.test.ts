import { describe, expect, it, vi } from "vitest";
vi.mock("$lib/daemon", () => ({ getSessionHistory: vi.fn() }));
vi.mock("$lib/chat/sessionController", () => ({ TRANSCRIPT_PAGE_SIZE: 100 }));
import { linkWorker } from "./workerLaneController";
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
