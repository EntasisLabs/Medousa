import { describe, expect, it } from "vitest";
import type { ItemProjection } from "$lib/forge";
import { reviewQueue, reviewQueueState } from "./reviewQueue";

function item(id: string, phase = "review", extra: Partial<ItemProjection> = {}): ItemProjection {
  return {
    id, title: id, human_phase: phase, state: "awaiting_review", brief: "", owner: "human",
    attempts: [{ id: "attempt-1", seq: 1, state: "sealed", evidence_id: "evidence" }],
    allowed_actions: { review: { allowed: phase === "review" }, apply: { allowed: false } } as ItemProjection["allowed_actions"],
    ...extra,
  };
}

describe("review queue", () => {
  it("includes prepared and historical revisions, excluding projects without changes to review", () => {
    expect(reviewQueueState(item("new", "work", { attempts: [] }))).toBeNull();
    expect(reviewQueueState(item("ready"))).toBe("ready");
    expect(reviewQueueState(item("working", "work"))).toBe("drafts");
    expect(reviewQueueState(item("done", "complete"))).toBe("reviewed");
  });

  it("does not mistake an earlier approval for approval of a newer attempt", () => {
    expect(reviewQueueState(item("new revision", "review", { review_decisions: [{ id: "old", strategy: "keep_checkout" }] }))).toBe("ready");
  });

  it("searches repository names, filters, and orders by saved activity", () => {
    const first = item("Older", "review", { updated_at: "2026-10-10T12:00:00Z", target: { kind: "git", repo_path: "/repo/Medousa", base_ref: "main" } });
    const second = item("Newer", "review", { updated_at: "2026-10-18T12:00:00Z", target: first.target });
    expect(reviewQueue([first, second], "ready", "medousa").map((row) => row.id)).toEqual(["Newer", "Older"]);
    expect(reviewQueue([first, second], "reviewed", "")).toEqual([]);
  });
});
