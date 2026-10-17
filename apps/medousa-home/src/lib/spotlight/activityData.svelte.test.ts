import { describe, expect, it, vi } from "vitest";
import { ActivityData, type ActivityDataPort } from "./activityData.svelte";
import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
const port = (): ActivityDataPort => ({ projects: vi.fn(async () => ({ items: [], nextCursor: null })), projectRuns: vi.fn(async () => []), schedules: vi.fn(async () => []), scheduleRuns: vi.fn(async () => []), card: vi.fn() });

describe("Activity request lifetime", () => {
  it("reads the catalog beyond the first ID-ordered page without loading every transcript", async () => {
    const api = port();
    api.projects = vi.fn().mockResolvedValueOnce({ items: [{ id: "a", brief: "x".repeat(4000) }], nextCursor: "a" }).mockResolvedValueOnce({ items: [{ id: "current" }], nextCursor: null });
    const data = new ActivityData(api); await data.refresh();
    expect(api.projects).toHaveBeenNthCalledWith(2, "a"); expect(data.projects.map((project) => project.id)).toEqual(["a", "current"]);
    expect(data.projects[0].brief).toHaveLength(1500); expect(data.nextCursor).toBeNull(); data.dispose();
  });
  it("enriches current work before historical failures", async () => {
    const api = port(); const data = new ActivityData(api); data.cardLimit = 1;
    const cards = [{ id: "old", column: "blocked", status_label: "dead_letter", updated_at_utc: "2026-10-01" }, { id: "live", column: "in_flight", status_label: "running", updated_at_utc: "2026-10-17" }] as WorkCard[];
    api.card = vi.fn(async (id) => ({ card: cards.find((card) => card.id === id) } as WorkCardDetail));
    await data.loadCards(cards, new Map());
    expect(api.card).toHaveBeenCalledExactlyOnceWith("live"); data.dispose();
  });
  it("drops a late workshop response after the lens closes", async () => {
    const api = port(); const response = deferred<Awaited<ReturnType<ActivityDataPort['projects']>>>();
    api.projects = vi.fn(() => response.promise);
    const data = new ActivityData(api); const pending = data.refresh(); data.dispose();
    response.resolve({ items: [{ id: "old-workshop" }] as never, nextCursor: "old" }); await pending;
    expect(data.projects).toEqual([]); expect(data.nextCursor).toBeNull(); expect(api.projectRuns).not.toHaveBeenCalled();
  });
  it("keeps other sources usable when Code is unavailable", async () => {
    const api = port(); api.projects = vi.fn().mockRejectedValue(new Error("offline"));
    api.schedules = vi.fn().mockResolvedValue([{ recurring_id: "schedule" }]);
    const data = new ActivityData(api); await data.refresh();
    expect(data.schedules).toHaveLength(1); expect(data.errors.Code).toBe("offline"); expect(data.loading).toBe(false); data.dispose();
  });
  it("deduplicates concurrent enrichment and never retains a late transcript", async () => {
    const api = port(); const response = deferred<WorkCardDetail>(); api.card = vi.fn(() => response.promise);
    const data = new ActivityData(api); const cards = [{ id: "a", updated_at_utc: "1", column: "in_flight" }] as WorkCard[];
    const pending = data.loadCards(cards, new Map()); await data.loadCards(cards, new Map());
    expect(api.card).toHaveBeenCalledOnce(); data.dispose(); response.resolve({ card: cards[0] } as WorkCardDetail); await pending;
    expect(data.details.size).toBe(0);
  });
});
