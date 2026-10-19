import { hostedReviewKey, listReviewProviders, queryHostedReviews, type HostedReview, type ReviewProviderDescriptor } from "./hostedReviews";

/** Small metadata-only queue, scoped to the connected workshop. File patches
 * belong to mounted review pages and never live in this store. */
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
  private repository: { provider: string; host: string; name: string; connectionId: string | null } | null = null;

  reset(scope: string) {
    if (scope === this.scope) return;
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
    if (this.connectionId === connectionId) return;
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
  async discover() {
    const generation = ++this.discoveryGeneration;
    try { const response = await listReviewProviders(); if (generation === this.discoveryGeneration) { this.providers = response.providers; this.error = response.problems.join(" · ") || null; } }
    catch (cause) { if (generation === this.discoveryGeneration) this.error = cause instanceof Error ? cause.message : String(cause); }
  }
  async inbox(folder: "authored" | "review_requested" | "participating" = "review_requested") {
    const generation = ++this.generation;
    this.folder = folder; this.repository = null; this.repositoryName = ""; this.nextCursor = null; this.reviews = []; this.loading = true; this.error = null;
    try {
      const provider = this.providers.find(p => p.id === this.accountProvider) ?? (this.providers.length === 1 ? this.providers[0] : null);
      if (!provider?.operations.includes("inbox")) return;
      const host = this.accountHost ?? provider.hosts[0];
      const result = await queryHostedReviews(provider.id, host, { operation: "inbox", folder, cursor: null }, this.connectionId);
      if (generation !== this.generation) return;
      if (result.kind !== "reviews") throw new Error("The package returned an unexpected inbox response.");
      for (const review of result.reviews) this.remember(review);
      this.nextCursor = result.next_cursor ?? null;
    } catch (cause) { if (generation === this.generation) this.error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (generation === this.generation) this.loading = false; }
  }
  async refresh() {
    if (this.folder === "repository" && this.repositoryName) await this.open(this.repositoryName, this.repository?.provider);
    else await this.inbox(this.folder === "repository" ? "review_requested" : this.folder);
  }
  async open(value: string, providerId?: string): Promise<HostedReview | null> {
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
      const result = await queryHostedReviews(provider.id, host, url ? { operation: "resolve", url: url.href } : { operation: "list", repository: value.trim(), cursor: null }, this.connectionId);
      if (generation !== this.generation) return null;
      if (result.kind === "review") { this.remember(result.review); return result.review; }
      if (result.kind !== "reviews") throw new Error("The package returned an unexpected review response.");
      this.folder = "repository"; this.repositoryName = value.trim(); this.reviews = [];
      this.repository = { provider: provider.id, host, name: value.trim(), connectionId: this.connectionId }; this.nextCursor = result.next_cursor ?? null;
      for (const review of result.reviews) this.remember(review);
      return null;
    } catch (cause) { if (generation === this.generation) this.error = cause instanceof Error ? cause.message : String(cause); return null; }
    finally { if (generation === this.generation) this.loading = false; }
  }
  async more() {
    if (!this.nextCursor || this.loading) return;
    const generation = this.generation; const repository = this.repository; this.loading = true; this.error = null;
    try {
      const provider = this.providers.find(p => p.id === this.accountProvider) ?? (this.providers.length === 1 ? this.providers[0] : null);
      const result = repository ? await queryHostedReviews(repository.provider, repository.host, { operation: "list", repository: repository.name, cursor: this.nextCursor }, repository.connectionId)
        : provider && this.folder !== "repository" ? await queryHostedReviews(provider.id, this.accountHost ?? provider.hosts[0], { operation: "inbox", folder: this.folder, cursor: this.nextCursor }, this.connectionId) : null;
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
