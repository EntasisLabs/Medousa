import { gitTargetRepoPath, type ItemProjection } from "$lib/forge";

export type ReviewQueueFilter = "all" | "ready" | "drafts" | "reviewed";

export function reviewQueueState(item: ItemProjection): Exclude<ReviewQueueFilter, "all"> | null {
  const hasEvidence = item.attempts?.some((attempt) => Boolean(attempt.evidence_id));
  if (!hasEvidence && item.human_phase !== "review" && !item.allowed_actions.review.allowed && !item.allowed_actions.apply.allowed) return null;
  if (item.human_phase === "complete" || ["accepted", "discarded"].includes(item.state) || item.allowed_actions.apply.allowed) return "reviewed";
  if (item.human_phase === "review" || item.allowed_actions.review.allowed) return "ready";
  return "drafts";
}

export function reviewRepository(item: ItemProjection): string {
  return gitTargetRepoPath(item.target)?.replaceAll("\\", "/").replace(/\/$/, "").split("/").at(-1) || "Project";
}

export function reviewQueue(items: ItemProjection[], filter: ReviewQueueFilter, query: string): ItemProjection[] {
  const needle = query.trim().toLowerCase();
  return items.filter((item) => {
    const state = reviewQueueState(item);
    return state && (filter === "all" || state === filter) && (!needle || `${item.title} ${reviewRepository(item)}`.toLowerCase().includes(needle));
  }).sort((a, b) => (Date.parse(b.updated_at || b.created_at || "") || 0) - (Date.parse(a.updated_at || a.created_at || "") || 0));
}
