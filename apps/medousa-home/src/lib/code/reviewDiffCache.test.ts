import { describe, expect, it } from "vitest";
import type { ReviewFileDiff } from "$lib/forge";
import { ReviewDiffCache, reviewDiffBytes, reviewDiffLines } from "./reviewDiffCache";

function diff(path: string, content = "before\nafter\n"): ReviewFileDiff {
  return {
    work_id: "work", attempt_id: "attempt", path, status: "modified",
    baseline_oid: "before", reviewed_oid: "after", binary: false,
    baseline: { exists: true, binary: false, byte_size: content.length, content },
    reviewed: { exists: true, binary: false, byte_size: content.length, content },
    hunks: [], changed_lines: [], truncated: false,
  };
}

describe("review comparison retention", () => {
  it("keeps only the most recently used comparisons under the byte budget", () => {
    const a = diff("a", "source\n".repeat(1000));
    const cache = new ReviewDiffCache(reviewDiffBytes(a) * 2, 12);
    cache.set("a", a);
    cache.set("b", { ...a, path: "b" });
    cache.get("a");
    expect(cache.set("c", { ...a, path: "c" })).toEqual(["b"]);
    expect(Object.keys(cache.snapshot())).toEqual(["a", "c"]);
  });

  it("bounds the number of open files even when their payloads are tiny", () => {
    const cache = new ReviewDiffCache(1024 * 1024, 12);
    for (let i = 0; i < 100; i++) cache.set(`${i}`, diff(`${i}`));
    expect(Object.keys(cache.snapshot())).toHaveLength(12);
    expect(cache.get("0")).toBeUndefined();
    expect(cache.get("99")?.path).toBe("99");
  });

  it("reaccounts replacements and releases data on close and review changes", () => {
    const original = diff("a");
    const cache = new ReviewDiffCache(reviewDiffBytes(original) * 2);
    cache.set("a", original);
    cache.set("a", original);
    expect(cache.set("b", diff("b"))).toEqual([]);
    cache.delete("a");
    expect(cache.get("a")).toBeUndefined();
    cache.clear();
    expect(cache.snapshot()).toEqual({});
    expect(cache.set("c", diff("c"))).toEqual([]);
  });

  it("leaves one oversized file readable while releasing every older comparison", () => {
    const cache = new ReviewDiffCache(2048);
    cache.set("a", diff("a"));
    expect(cache.set("huge", diff("huge", "x".repeat(4096)))).toEqual(["a"]);
    expect(Object.keys(cache.snapshot())).toEqual(["huge"]);
  });

  it("shares full-file context arrays across symbol previews without sharing revisions", () => {
    const comparison = diff("a");
    expect(reviewDiffLines(comparison)).toBe(reviewDiffLines(comparison));
    expect(reviewDiffLines(comparison).before).toEqual(["before", "after"]);
    const revised = diff("a", "updated\n");
    expect(reviewDiffLines(revised).after).toEqual(["updated"]);
    expect(reviewDiffLines(revised)).not.toBe(reviewDiffLines(comparison));
  });
});
