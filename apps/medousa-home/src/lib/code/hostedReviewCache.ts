import type { HostedReviewRef, ReviewProviderOperation, ReviewProviderResult } from "$lib/types/generated/daemon_api";

export type ReviewCacheContext = { scope: string; provider: string; host: string; connectionId: string | null };
type Entry = { context: ReviewCacheContext; request: ReviewProviderOperation; result: ReviewProviderResult; saved: number; used: number; bytes: number };
export type ReviewCacheOptions = { force?: boolean; onCached?: (result: ReviewProviderResult) => void };
const MiB = 1024 * 1024;

function stable(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stable).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.entries(value).filter(([, v]) => v !== undefined).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${JSON.stringify(k)}:${stable(v)}`).join(",")}}`;
  return JSON.stringify(value);
}
function key(context: ReviewCacheContext, request: ReviewProviderOperation) { return stable([context, request]); }
function sameReview(a: HostedReviewRef, b: HostedReviewRef) {
  return a.provider_id === b.provider_id && a.host === b.host && a.repository === b.repository && a.native_id === b.native_id;
}
function lifetime(request: ReviewProviderOperation) {
  if (request.operation === "files" || request.operation === "diff") return Infinity;
  if (request.operation === "checks" || request.operation === "check_details") return 20_000;
  return 60_000;
}

/** Window-local response data, never mounted pages or parsed diff trees.
 * Estimates deliberately allow for UTF-16 strings and object overhead; these
 * budgets describe retained payloads, not an exact browser heap measurement. */
export class HostedReviewCache {
  private entries = new Map<string, Entry>();
  private pending = new Map<string, { context: ReviewCacheContext; request: ReviewProviderOperation; promise: Promise<ReviewProviderResult> }>();
  private bytes = 0;
  private patchBytes = 0;
  constructor(private maxBytes = 12 * MiB, private maxPatchBytes = 4 * MiB, private maxEntries = 128, private now = () => Date.now()) {}

  peek(context: ReviewCacheContext, request: ReviewProviderOperation) {
    this.prune();
    const id = key(context, request), entry = this.entries.get(id);
    if (!entry) return null;
    entry.used = this.now(); this.entries.delete(id); this.entries.set(id, entry);
    return { result: entry.result, fresh: this.now() - entry.saved < lifetime(request) };
  }
  query(context: ReviewCacheContext, request: ReviewProviderOperation, load: () => Promise<ReviewProviderResult>, options: ReviewCacheOptions = {}): Promise<ReviewProviderResult> {
    const cached = options.force ? null : this.peek(context, request);
    if (cached) {
      options.onCached?.(cached.result);
    }
    const id = key(context, request), pending = this.pending.get(id);
    if (pending) return pending.promise;
    if (cached?.fresh) return Promise.resolve(cached.result);
    const task = { context: { ...context }, request, promise: null as unknown as Promise<ReviewProviderResult> };
    let response: Promise<ReviewProviderResult>;
    try { response = load(); } catch (cause) { return Promise.reject(cause); }
    task.promise = response.then(result => {
      // Invalidation detaches pending work so a late response cannot restore it.
      if (this.pending.get(id) === task) this.put(context, request, result);
      return result;
    }).finally(() => { if (this.pending.get(id) === task) this.pending.delete(id); });
    this.pending.set(id, task);
    return task.promise;
  }
  invalidateReview(context: Omit<ReviewCacheContext, "scope"> & { scope?: string }, review?: HostedReviewRef) {
    const matches = (entry: { context: ReviewCacheContext; request: ReviewProviderOperation }) => {
      const ctx = entry.context;
      if ((context.scope !== undefined && ctx.scope !== context.scope) || ctx.provider !== context.provider || ctx.host !== context.host || ctx.connectionId !== context.connectionId) return false;
      // Inbox/repository/resolve results can contain the changed review too.
      return !review || !("review" in entry.request) || sameReview(entry.request.review, review);
    };
    for (const [id, entry] of this.entries) if (matches(entry)) this.remove(id);
    for (const [id, entry] of this.pending) if (matches(entry)) this.pending.delete(id);
  }
  clear() { this.entries.clear(); this.pending.clear(); this.bytes = this.patchBytes = 0; }
  get usage() { this.prune(); return { bytes: this.bytes, patchBytes: this.patchBytes, entries: this.entries.size }; }

  private put(context: ReviewCacheContext, request: ReviewProviderOperation, result: ReviewProviderResult) {
    this.prune();
    if (result.kind === "review") {
      const { reference, head_revision } = result.review;
      const previous = this.entries.get(key(context, { operation: "get", review: reference }))?.result;
      const baseChanged = previous?.kind !== "review" || previous.review.base_revision !== result.review.base_revision;
      const outdated = (entry: { context: ReviewCacheContext; request: ReviewProviderOperation }) => stable(entry.context) === stable(context) && "review" in entry.request && sameReview(entry.request.review, reference) && "revision" in entry.request && (entry.request.revision !== head_revision || (baseChanged && ["files", "diff"].includes(entry.request.operation)));
      for (const [id, entry] of this.entries) if (outdated(entry)) this.remove(id);
      for (const [id, entry] of this.pending) if (outdated(entry)) this.pending.delete(id);
    }
    const id = key(context, request);
    this.remove(id);
    const patch = request.operation === "diff";
    const bytes = (JSON.stringify(result).length + id.length) * 4 + 1024;
    if (bytes > Math.min(this.maxBytes, patch ? Math.min(this.maxPatchBytes, MiB) : 4 * MiB)) return;
    this.entries.set(id, { context: { ...context }, request, result, saved: this.now(), used: this.now(), bytes });
    this.bytes += bytes; if (patch) this.patchBytes += bytes;
    while (this.patchBytes > this.maxPatchBytes) this.remove([...this.entries].find(([, entry]) => entry.request.operation === "diff")![0]);
    while (this.bytes > this.maxBytes || this.entries.size > this.maxEntries) this.remove(this.entries.keys().next().value!);
    if (request.operation === "resolve" && result.kind === "review") this.put(context, { operation: "get", review: result.review.reference }, result);
  }
  private prune() { for (const [id, entry] of this.entries) if (this.now() - entry.used > 15 * 60_000) this.remove(id); }
  private remove(id: string) {
    const entry = this.entries.get(id);
    if (!entry) return;
    this.bytes -= entry.bytes; if (entry.request.operation === "diff") this.patchBytes -= entry.bytes;
    this.entries.delete(id);
  }
}
export const hostedReviewCache = new HostedReviewCache();
