import { hostedReviewKey, listReviewProviders, queryHostedReviews, type HostedReview, type ReviewProviderDescriptor } from "./hostedReviews";
import { hostedReviewCache, type ReviewCacheOptions } from "./hostedReviewCache";
import type { ReviewProviderOperation, ReviewProviderResult } from "$lib/types/generated/daemon_api";

/** Small metadata-only queue. The separate bounded response cache owns payloads. */
export class HostedReviews {
  connectionId = $state<string | null>(null);
  accountProvider = $state<string | null>(null);
  accountHost = $state<string | null>(null);
  providers = $state<ReviewProviderDescriptor[]>([]);
  reviews = $state<HostedReview[]>([]);
  error = $state<string | null>(null);
  loading = $state(false);
  scope = $state("");
  folder = $state<"authored" | "review_requested" | "participating" | "repository">("review_requested");
  repositoryName = $state("");
  nextCursor = $state<string | null>(null);
  private generation = 0;
  private discoveryGeneration = 0;
  private discoveredAt = 0;
  private discovering: Promise<void> | null = null;
  private queueKey: string | null = null;
  private repository: { provider: string; host: string; name: string; connectionId: string | null } | null = null;

  reset(scope: string) {
    if (scope === this.scope) return;
    hostedReviewCache.clear(); this.discoveredAt = 0; this.discovering = null; this.queueKey = null;
    this.connectionId = null; this.accountProvider = null; this.accountHost = null;
    try {
      const saved = typeof localStorage === "undefined" ? null : JSON.parse(localStorage.getItem(`medousa.review-account:${scope}`) || "null");
      if (saved && typeof saved.id === "string" && typeof saved.provider === "string" && typeof saved.host === "string") {
        this.connectionId = saved.id; this.accountProvider = saved.provider; this.accountHost = saved.host;
      }
    } catch { /* A stale preference must not prevent opening Reviews. */ }
    this.generation++; this.discoveryGeneration++; this.scope = scope; this.providers = []; this.reviews = []; this.error = null; this.loading = false; this.repository = null; this.repositoryName = ""; this.folder = "review_requested"; this.nextCursor = null;
  }
  selectAccount(connectionId: string | null, provider: string | null = null, host: string | null = null) {
    if (this.connectionId === connectionId && this.accountProvider === provider && this.accountHost === host) return;
    hostedReviewCache.clear(); this.queueKey = null;
    this.connectionId = connectionId; this.accountProvider = provider; this.accountHost = host;
    try {
      if (typeof localStorage !== "undefined") {
        const key = `medousa.review-account:${this.scope}`;
        if (connectionId && provider && host) localStorage.setItem(key, JSON.stringify({id:connectionId,provider,host}));
        else localStorage.removeItem(key);
      }
    } catch { /* In-memory account selection still works if storage is unavailable. */ }
    this.generation++; this.reviews = []; this.repository = null; this.repositoryName = ""; this.folder = "review_requested"; this.nextCursor = null; this.error = null; this.loading = false;
  }
  async discover(force = false) {
    if (!force && this.discoveredAt && Date.now() - this.discoveredAt < 60_000) return;
    if (this.discovering) return this.discovering;
    const generation = ++this.discoveryGeneration;
    const task = (async () => {
      try { const response = await listReviewProviders(); if (generation === this.discoveryGeneration) { this.providers = response.providers; this.error = response.problems.join(" · ") || null; this.discoveredAt = Date.now(); } }
      catch (cause) { if (generation === this.discoveryGeneration) this.error = cause instanceof Error ? cause.message : String(cause); }
      finally { if (generation === this.discoveryGeneration) this.discovering = null; }
    })();
    this.discovering = task;
    return task;
  }
  private query(provider: string, host: string, request: ReviewProviderOperation, options?: ReviewCacheOptions) {
    const connectionId = this.connectionId;
    return hostedReviewCache.query({ scope: this.scope, provider, host, connectionId }, request, () => queryHostedReviews(provider, host, request, connectionId), options);
  }
  async inbox(folder: "authored" | "review_requested" | "participating" = "review_requested", force = false) {
    const generation = ++this.generation;
    const sameFolder = this.folder === folder && !this.repository;
    this.folder = folder; this.repository = null; this.repositoryName = "";
    if (!sameFolder) { this.nextCursor = null; this.reviews = []; }
    this.loading = !sameFolder || !this.reviews.length; this.error = null;
    try {
      const provider = this.providers.find(p => p.id === this.accountProvider) ?? (this.providers.length === 1 ? this.providers[0] : null);
      if (!provider?.operations.includes("inbox")) return;
      const host = this.accountHost ?? provider.hosts[0];
      const request = { operation: "inbox" as const, folder, cursor: null };
      const context = { scope: this.scope, provider: provider.id, host, connectionId: this.connectionId };
      const id = JSON.stringify(context) + folder;
      if (!force && sameFolder && this.queueKey === id && hostedReviewCache.peek(context, request)?.fresh) return;
      const apply = (result: ReviewProviderResult) => {
        if (generation !== this.generation) return;
        if (result.kind !== "reviews") throw new Error("The package returned an unexpected inbox response.");
        this.reviews = []; for (const review of result.reviews) this.remember(review);
        this.nextCursor = result.next_cursor ?? null; this.queueKey = id; this.loading = false;
      };
      const result = await this.query(provider.id, host, request, { force, onCached: apply });
      apply(result);
    } catch (cause) { if (generation === this.generation) this.error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (generation === this.generation) this.loading = false; }
  }
  async refresh(force = false) {
    if (this.folder === "repository" && this.repositoryName) await this.open(this.repositoryName, this.repository?.provider, force);
    else await this.inbox(this.folder === "repository" ? "review_requested" : this.folder, force);
  }
  async open(value: string, providerId?: string, force = false): Promise<HostedReview | null> {
    value = value.trim();
    const generation = ++this.generation;
    this.loading = true; this.error = null;
    try {
      const url = value.startsWith("https://") ? new URL(value) : null;
      providerId = providerId || this.accountProvider || undefined;
      const provider = providerId ? this.providers.find((p) => p.id === providerId) : this.providers.find((p) => p.hosts.includes(url?.hostname ?? "")) ?? (this.providers.length === 1 ? this.providers[0] : null);
      if (!provider) throw new Error("Choose an installed review provider first.");
      const host = url?.hostname ?? this.accountHost ?? provider.hosts[0];
      if (!host) throw new Error("This provider needs a review URL with a hostname.");
      const request = url ? { operation: "resolve" as const, url: url.href } : { operation: "list" as const, repository: value.trim(), cursor: null };
      const apply = (result: ReviewProviderResult) => {
        if (generation !== this.generation || result.kind !== "reviews") return;
        this.folder = "repository"; this.repositoryName = value.trim(); this.reviews = [];
        this.repository = { provider: provider!.id, host, name: value.trim(), connectionId: this.connectionId }; this.nextCursor = result.next_cursor ?? null;
        for (const review of result.reviews) this.remember(review);
        this.loading = false;
      };
      const result = await this.query(provider.id, host, request, { force, onCached: apply });
      if (generation !== this.generation) return null;
      if (result.kind === "review") { this.remember(result.review); return result.review; }
      if (result.kind !== "reviews") throw new Error("The package returned an unexpected review response.");
      apply(result);
      return null;
    } catch (cause) { if (generation === this.generation) this.error = cause instanceof Error ? cause.message : String(cause); return null; }
    finally { if (generation === this.generation) this.loading = false; }
  }
  async more() {
    if (!this.nextCursor || this.loading) return;
    const generation = this.generation; const repository = this.repository; this.loading = true; this.error = null;
    try {
      const provider = this.providers.find(p => p.id === this.accountProvider) ?? (this.providers.length === 1 ? this.providers[0] : null);
      const result = repository ? await this.query(repository.provider, repository.host, { operation: "list", repository: repository.name, cursor: this.nextCursor })
        : provider && this.folder !== "repository" ? await this.query(provider.id, this.accountHost ?? provider.hosts[0], { operation: "inbox", folder: this.folder, cursor: this.nextCursor }) : null;
      if (!result) return;
      if (generation !== this.generation) return;
      if (result.kind !== "reviews") throw new Error("The package returned an unexpected review response.");
      for (const review of result.reviews) this.remember(review);
      this.nextCursor = this.reviews.length >= 500 ? null : result.next_cursor ?? null;
    } catch (cause) { if (generation === this.generation) this.error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (generation === this.generation) this.loading = false; }
  }
  private remember(review: HostedReview) {
    const key = hostedReviewKey(review.reference);
    this.reviews = [{ ...review, body: "" }, ...this.reviews.filter((r) => hostedReviewKey(r.reference) !== key)]
      .sort((a, b) => (Date.parse(b.updated_at) || 0) - (Date.parse(a.updated_at) || 0)).slice(0, 500);
  }
}
export const hostedReviews = new HostedReviews();
