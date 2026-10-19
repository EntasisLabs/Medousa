import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemProjection, ReviewProjection } from "$lib/forge";

const { getReview, getUndertaking } = vi.hoisted(() => ({ getReview: vi.fn(), getUndertaking: vi.fn() }));
vi.mock("$lib/forge", () => ({ getReview, getUndertaking }));
import { createReviewPage, reviewReadingState } from "./reviewPage.svelte";

const detail = { id: "work", title: "A change" } as ItemProjection;
const projection = (attemptId: string) => ({ work_id: "work", attempt_id: attemptId, changed_files: [{ path: "src/main.rs" }] } as ReviewProjection);

describe("focused review ownership", () => {
  beforeEach(() => { vi.clearAllMocks(); getUndertaking.mockResolvedValue(detail); });

  it("pins the revision across refreshes and navigation, scoped to the workshop", async () => {
    getReview.mockResolvedValue(projection("attempt-2"));
    const page = createReviewPage("work", "pin-test");
    await page.refresh();
    expect(page.reading.lens).toBe("overview");
    page.reading.lens = "changes";
    page.reading.directory = "src";
    page.reading.scroll.changes = 430;
    page.dispose();
    const reopened = createReviewPage("work", "pin-test");
    await reopened.refresh();
    expect(getReview).toHaveBeenLastCalledWith("work", "attempt-2");
    expect(reopened.reading).toMatchObject({ lens: "changes", directory: "src", scroll: { changes: 430 } });
    expect(reviewReadingState("another-workshop", "work").attemptId).toBeNull();
  });

  it("ignores an older request that finishes after a revision switch", async () => {
    let resolveOld!: (review: ReviewProjection) => void;
    getReview.mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve; }));
    const page = createReviewPage("work", "race-test");
    const old = page.refresh();
    getReview.mockResolvedValueOnce(projection("attempt-3"));
    await page.selectCandidate("attempt-3");
    resolveOld(projection("attempt-1"));
    await old;
    expect(page.review?.attempt_id).toBe("attempt-3");
    expect(page.reading.attemptId).toBe("attempt-3");
  });

  it("keeps the selected revision and position when another revision fails to load", async () => {
    getReview.mockResolvedValueOnce(projection("attempt-1"));
    const page = createReviewPage("work", "failure-test");
    await page.refresh();
    page.reading.scroll.overview = 310;
    getReview.mockRejectedValueOnce(new Error("Workshop disconnected"));
    await page.selectCandidate("attempt-2");
    expect(page.error).toBe("Workshop disconnected");
    expect(page.review?.attempt_id).toBe("attempt-1");
    expect(page.reading.scroll.overview).toBe(310);
  });

  it("does not let background refresh replace an explicit revision selection", async () => {
    let resolve!: (review: ReviewProjection) => void;
    getReview.mockResolvedValueOnce(projection("attempt-1"));
    const page = createReviewPage("work", "poll-test");
    await page.refresh();
    getReview.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const selecting = page.selectCandidate("attempt-2");
    await page.refresh();
    expect(getReview).toHaveBeenCalledTimes(2);
    resolve(projection("attempt-2"));
    await selecting;
    expect(page.reading.attemptId).toBe("attempt-2");
  });

  it("clears a directory filter when the new revision no longer contains those files", async () => {
    getReview.mockResolvedValueOnce(projection("attempt-1"));
    const page = createReviewPage("work", "directory-test");
    await page.refresh();
    page.reading.directory = "src";
    getReview.mockResolvedValueOnce({ ...projection("attempt-2"), changed_files: [{ path: "README.md" }] });
    await page.selectCandidate("attempt-2");
    expect(page.reading.directory).toBeNull();
  });

  it("does not repopulate a page after it closes", async () => {
    let resolve!: (review: ReviewProjection) => void;
    getReview.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const page = createReviewPage("work", "dispose-test");
    const pending = page.refresh();
    page.dispose();
    resolve(projection("attempt-1"));
    await pending;
    expect(page.review).toBeNull();
  });
});
