import { expect, it, vi } from "vitest";
import { HostedReviewCache } from "./hostedReviewCache";
import type { ReviewProviderOperation, ReviewProviderResult } from "$lib/types/generated/daemon_api";
const context = { scope: "workshop", provider: "example", host: "git.example.test", connectionId: "account" };
const reference = { provider_id: context.provider, host: context.host, repository: "team/repo", native_id: "1" };
const get = { operation: "get" as const, review: reference };
const diff = (path: string, revision = "abc"): ReviewProviderOperation => ({ operation: "diff", review: reference, revision, path });
const patch = (path: string, text = "small patch"): ReviewProviderResult => ({ kind: "diff", path, patch: text });
const details = (head_revision = "abc"): ReviewProviderResult => ({ kind: "review", review: { reference, title: "Review", body: "Body", state: "open", native_state: "open", actions: [], base_revision: "base", author: "author", head_revision, head_branch: "topic", base_branch: "main", changed_files: 1, additions: 1, deletions: 1, url: "https://git.example.test/team/repo/1", updated_at: "2026-10-19" } });

it("deduplicates pending reads, returns fresh data without a request, and revalidates stale data", async () => {
  let time = 1, finish!: (value: ReviewProviderResult) => void;
  const cache = new HostedReviewCache(undefined, undefined, undefined, () => time);
  const load = vi.fn(() => new Promise<ReviewProviderResult>(resolve => finish = resolve));
  const first = cache.query(context, get, load), second = cache.query(context, get, load);
  expect(first).toBe(second); expect(load).toHaveBeenCalledTimes(1);
  finish(details()); await first;
  const onCached = vi.fn();
  await cache.query(context, get, load, { onCached }); expect(load).toHaveBeenCalledTimes(1); expect(onCached).toHaveBeenCalledOnce();
  time += 61_000;
  const refresh = cache.query(context, get, load, { onCached });
  expect(onCached).toHaveBeenCalledTimes(2); expect(load).toHaveBeenCalledTimes(2);
  finish(details("new")); await refresh; expect(cache.peek(context, get)?.fresh).toBe(true);
});

it("does not alias workshops, accounts, providers, hosts, revisions, paths, or page cursors", async () => {
  const cache = new HostedReviewCache(), load = vi.fn().mockResolvedValue(patch("a"));
  await cache.query(context, diff("a"), load);
  for (const changed of [{ ...context, scope: "other" }, { ...context, connectionId: "other" }, { ...context, provider: "other" }, { ...context, host: "other.test" }]) expect(cache.peek(changed, diff("a"))).toBeNull();
  expect(cache.peek(context, diff("b"))).toBeNull(); expect(cache.peek(context, diff("a", "new"))).toBeNull();
  await cache.query(context, { operation: "discussion", review: reference, cursor: null }, vi.fn().mockResolvedValue({ kind: "discussion", entries: [] }));
  expect(cache.peek(context, { operation: "discussion", review: reference, cursor: "2" })).toBeNull();
});

it("shares an explicit refresh with concurrent readers even while an older cached result is fresh", async () => {
  const cache = new HostedReviewCache();
  await cache.query(context, get, vi.fn().mockResolvedValue(details()));
  let finish!: (value: ReviewProviderResult) => void;
  const refresh = cache.query(context, get, () => new Promise(resolve => finish = resolve), { force: true });
  const load = vi.fn(), onCached = vi.fn();
  const reader = cache.query(context, get, load, { onCached });
  expect(reader).toBe(refresh); expect(onCached).toHaveBeenCalledOnce(); expect(load).not.toHaveBeenCalled();
  finish(details("new")); expect(await reader).toEqual(details("new"));
});

it("evicts least recently used payloads and patches under independent hard budgets", async () => {
  const cache = new HostedReviewCache(22_000, 13_000, 3);
  await cache.query(context, get, vi.fn().mockResolvedValue(details()));
  for (const path of ["a", "b", "c", "d"]) await cache.query(context, diff(path), vi.fn().mockResolvedValue(patch(path, "x".repeat(1800))));
  expect(cache.usage.bytes).toBeLessThanOrEqual(22_000); expect(cache.usage.patchBytes).toBeLessThanOrEqual(13_000); expect(cache.usage.entries).toBeLessThanOrEqual(3);
  expect(cache.peek(context, diff("a"))).toBeNull(); expect(cache.peek(context, diff("d"))).not.toBeNull(); expect(cache.peek(context, get)).not.toBeNull();
  const countCache = new HostedReviewCache(100_000, 100_000, 2);
  await countCache.query(context, diff("a"), vi.fn().mockResolvedValue(patch("a")));
  await countCache.query(context, diff("b"), vi.fn().mockResolvedValue(patch("b")));
  countCache.peek(context, diff("a"));
  await countCache.query(context, diff("c"), vi.fn().mockResolvedValue(patch("c")));
  expect(countCache.peek(context, diff("b"))).toBeNull();
});

it("allows an oversized patch to be read without retaining it", async () => {
  const cache = new HostedReviewCache(), result = patch("huge", "x".repeat(300_000));
  expect(await cache.query(context, diff("huge"), vi.fn().mockResolvedValue(result))).toBe(result);
  expect(cache.usage).toEqual({ bytes: 0, patchBytes: 0, entries: 0 });
});

it("never restores invalidated data from a late response and bypasses cache on explicit refresh", async () => {
  const cache = new HostedReviewCache(); let finish!: (value: ReviewProviderResult) => void;
  const old = cache.query(context, get, () => new Promise(resolve => finish = resolve));
  cache.invalidateReview(context, reference);
  await cache.query(context, get, vi.fn().mockResolvedValue(details("new")), { force: true });
  finish(details("old")); await old;
  expect(cache.peek(context, get)?.result).toEqual(details("new"));
  const load = vi.fn().mockResolvedValue(details("forced"));
  await cache.query(context, get, load, { force: true }); expect(load).toHaveBeenCalledOnce();
});

it("expires mutable checks while retaining revision-pinned patches, and drops old revisions", async () => {
  let time = 1; const cache = new HostedReviewCache(undefined, undefined, undefined, () => time);
  const checks = { operation: "checks" as const, review: reference, revision: "abc" };
  await cache.query(context, checks, vi.fn().mockResolvedValue({ kind: "checks", checks: [] }));
  await cache.query(context, diff("a"), vi.fn().mockResolvedValue(patch("a")));
  time += 21_000; expect(cache.peek(context, checks)?.fresh).toBe(false); expect(cache.peek(context, diff("a"))?.fresh).toBe(true);
  await cache.query(context, get, vi.fn().mockResolvedValue(details("new")));
  expect(cache.peek(context, diff("a"))).toBeNull(); expect(cache.peek(context, checks)).toBeNull();
  time += 16 * 60_000; expect(cache.usage.entries).toBe(0);
});

it("does not cache errors, and clearing also detaches unfinished work", async () => {
  const cache = new HostedReviewCache();
  await expect(cache.query(context, get, vi.fn().mockRejectedValue(new Error("offline")))).rejects.toThrow("offline");
  expect(cache.peek(context, get)).toBeNull();
  let finish!: (value: ReviewProviderResult) => void;
  const pending = cache.query(context, get, () => new Promise(resolve => finish = resolve));
  cache.clear(); finish(details()); await pending; expect(cache.usage.entries).toBe(0);
});

it("invalidates related inboxes after writes and reuses resolved review details", async () => {
  const cache = new HostedReviewCache();
  await cache.query(context, { operation: "resolve", url: "https://git.example.test/team/repo/1" }, vi.fn().mockResolvedValue(details()));
  expect(cache.peek(context, get)?.fresh).toBe(true);
  const inbox = { operation: "inbox" as const, folder: "authored" as const };
  await cache.query(context, inbox, vi.fn().mockResolvedValue({ kind: "reviews", reviews: [] }));
  cache.invalidateReview(context, reference); expect(cache.peek(context, get)).toBeNull(); expect(cache.peek(context, inbox)).toBeNull();
});

it("discards comparisons when the base commit changes without a new head commit", async () => {
  const cache = new HostedReviewCache();
  await cache.query(context, get, vi.fn().mockResolvedValue(details()));
  await cache.query(context, diff("a"), vi.fn().mockResolvedValue(patch("a")));
  const next = details(); if (next.kind === "review") next.review.base_revision = "another-base";
  await cache.query(context, get, vi.fn().mockResolvedValue(next), { force: true });
  expect(cache.peek(context, diff("a"))).toBeNull();
});
