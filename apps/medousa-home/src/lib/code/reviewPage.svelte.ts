import { reviewFileInDirectory } from "$lib/code/reviewAreas";
import { getReview, getUndertaking, type ItemProjection, type ReviewProjection } from "$lib/forge";

export type ReviewLens = "overview" | "changes";
export type ReviewReadingState = {
  attemptId: string | null;
  lens: ReviewLens;
  directory: string | null;
  scroll: Record<ReviewLens, number>;
  diff: { comparison: string; expanded: string[]; mode: "inline" | "side"; wrap: boolean; density: "comfortable" | "compact" };
};

// Keep navigation state, never file diffs or command logs, after a page unmounts.
const readingStates = new Map<string, ReviewReadingState>();
const MAX_READING_STATES = 32;

export function reviewReadingState(scope: string, workId: string): ReviewReadingState {
  const key = JSON.stringify([scope, workId]);
  let state = readingStates.get(key);
  if (!state) {
    const fresh = $state<ReviewReadingState>({ attemptId: null, lens: "overview", directory: null, scroll: { overview: 0, changes: 0 }, diff: { comparison: "", expanded: [], mode: "inline", wrap: false, density: "compact" } });
    state = fresh;
    readingStates.set(key, state);
  }
  readingStates.delete(key);
  readingStates.set(key, state);
  if (readingStates.size > MAX_READING_STATES) readingStates.delete(readingStates.keys().next().value!);
  return state;
}

/** A review owns its projection; selecting Code or another pane cannot replace it. */
export function createReviewPage(workId: string, scope: string) {
  const reading = reviewReadingState(scope, workId);
  let detail = $state<ItemProjection | null>(null);
  let review = $state<ReviewProjection | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let generation = 0;
  let selectingRevision = false;

  async function refresh(attemptId = reading.attemptId, explicit = false) {
    if (selectingRevision && !explicit) return false;
    const request = ++generation;
    loading = true;
    error = null;
    try {
      const [nextDetail, nextReview] = await Promise.all([
        getUndertaking(workId), getReview(workId, attemptId ?? undefined),
      ]);
      if (request !== generation) return false;
      if (JSON.stringify(detail) !== JSON.stringify(nextDetail)) detail = nextDetail;
      if (JSON.stringify(review) !== JSON.stringify(nextReview)) review = nextReview;
      // Pin the revision being read, including while newer attempts arrive.
      reading.attemptId = nextReview.attempt_id ?? null;
      if (reading.directory !== null && !nextReview.changed_files.some((file) => reviewFileInDirectory(file.path, reading.directory))) reading.directory = null;
      return true;
    } catch (cause) {
      if (request === generation) error = cause instanceof Error ? cause.message : String(cause);
      return false;
    } finally {
      if (request === generation) loading = false;
    }
  }

  async function selectCandidate(attemptId: string) {
    if (attemptId === reading.attemptId) return;
    selectingRevision = true;
    try {
      if (await refresh(attemptId, true)) reading.scroll = { overview: 0, changes: 0 };
    } finally { selectingRevision = false; }
  }

  return {
    reading,
    get detail() { return detail; },
    get review() { return review; },
    get loading() { return loading; },
    get error() { return error; },
    refresh,
    selectCandidate,
    dispose() { generation += 1; },
  };
}
