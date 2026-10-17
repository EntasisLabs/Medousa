import { beforeEach, describe, expect, it, vi } from "vitest";
import { getWorkspaceCard, cancelWorkspaceCard } from "$lib/daemon";
import { WorkspaceStore } from "./workspace.svelte";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";

vi.mock("$lib/daemon", () => ({ getWorkspaceCard: vi.fn(), cancelWorkspaceCard: vi.fn(), retryWorkspaceCard: vi.fn(), archiveAskJob: vi.fn(), archiveWorkspaceCard: vi.fn(), enqueueDaemonAsk: vi.fn(), fetchWorkspaceSnapshot: vi.fn() }));
vi.mock("$lib/notifications", () => ({ presentHomeNotification: vi.fn() }));
vi.mock("$lib/work/workerTranscripts.svelte", () => ({ workerTranscripts: { ingestDetail: vi.fn(), ingestProgress: vi.fn() } }));
const card = (id: string): WorkCard => ({ id, title: id, column: "in_flight", status_label: "running", created_at_utc: "2026-10-17", updated_at_utc: "2026-10-17" });
const detail = (id: string): WorkCardDetail => ({ card: card(id), kind: "stasis_job", terminal: false, wrapping_up_reasons: [], associations: { vault_paths: [], artifact_ids: [], locus_node_ids: [] }, live_output: id });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
beforeEach(() => { vi.clearAllMocks(); vi.mocked(getWorkspaceCard).mockImplementation(async (id) => detail(id)); });

describe("shared task selection", () => {
  it("ignores a late task A after selecting B and does not retain selected output globally", async () => {
    const a = deferred<WorkCardDetail>(); vi.mocked(getWorkspaceCard).mockImplementationOnce(() => a.promise);
    const store = new WorkspaceStore(); const pending = store.selectCard("a"); await store.selectCard("b");
    a.resolve(detail("a")); await pending;
    expect(store.selectedCardDetail?.card.id).toBe("b"); expect(store.cardDetailsCache.size).toBe(0);
  });
  it("does not reopen or refill task details after closing", async () => {
    const response = deferred<WorkCardDetail>(); vi.mocked(getWorkspaceCard).mockReturnValueOnce(response.promise);
    const store = new WorkspaceStore(); const pending = store.selectCard("a"); store.clearSelection();
    response.resolve(detail("a")); await pending;
    expect(store.selectedCardId).toBeNull(); expect(store.selectedCardDetail).toBeNull(); expect(store.cardDetailsCache.size).toBe(0);
  });
  it("isolates late responses across workshops even for the same task ID", async () => {
    const old = deferred<WorkCardDetail>(); vi.mocked(getWorkspaceCard).mockReturnValueOnce(old.promise);
    const store = new WorkspaceStore(); const pending = store.selectCard("same"); store.resetForWorkshopSwitch(); await store.selectCard("same");
    old.resolve({ ...detail("same"), live_output: "old workshop" }); await pending;
    expect(store.selectedCardDetail?.live_output).toBe("same");
  });
  it("refreshes past an existing summary cache", async () => {
    const store = new WorkspaceStore(); store.cardDetailsCache.set("a", { ...detail("a"), live_output: "old" });
    await store.selectCard("a"); expect(getWorkspaceCard).not.toHaveBeenCalled();
    await store.refreshSelectedCard(); expect(getWorkspaceCard).toHaveBeenCalledWith("a"); expect(store.selectedCardDetail?.live_output).toBe("a");
  });
  it("does not show an old cancel response on another task", async () => {
    const response = deferred<Awaited<ReturnType<typeof cancelWorkspaceCard>>>(); vi.mocked(cancelWorkspaceCard).mockReturnValueOnce(response.promise);
    const store = new WorkspaceStore(); await store.selectCard("a"); const pending = store.cancelSelectedCard(); await store.selectCard("b");
    response.resolve({ ok: false, message: "old cancellation" } as Awaited<ReturnType<typeof cancelWorkspaceCard>>); await pending;
    expect(store.cardActionMessage).toBeNull(); expect(store.cardDetailError).toBeNull();
  });
});

describe("task completion delivery", () => {
  it("queues close completions, dismisses only the current job, and resets with the workshop", async () => {
    const store = new WorkspaceStore(); const a = card("medousa-daemon-ask-a"); const b = card("medousa-daemon-ask-b");
    store.applyEvent({ workspace_revision: 1, stream_event_type: "snapshot", emitted_at_utc: "", snapshot: { workspace_revision: 1, server_time_utc: "", cards: [a, b], counts_by_column: {}, feed_tail: [] } });
    for (const entry of [a, b]) store.applyEvent({ workspace_revision: 2, stream_event_type: "card_upserted", emitted_at_utc: "", card: { ...entry, column: "done", status_label: "succeeded" } });
    expect(store.pendingAskCompletion?.jobId).toBe(a.id);
    store.clearPendingAskCompletion(b.id); expect(store.pendingAskCompletion?.jobId).toBe(a.id);
    store.clearPendingAskCompletion(a.id); expect(store.pendingAskCompletion?.jobId).toBe(b.id);
    store.clearPendingAskCompletion(a.id); expect(store.pendingAskCompletion?.jobId).toBe(b.id);
    store.resetForWorkshopSwitch(); expect(store.pendingAskCompletion).toBeNull();
    store.clearPendingAskCompletion(); expect(store.pendingAskCompletion).toBeNull();
    await Promise.resolve();
  });
});
