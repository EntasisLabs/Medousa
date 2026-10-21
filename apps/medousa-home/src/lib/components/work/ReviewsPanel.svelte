<script lang="ts">
  import { hostedReviews } from "$lib/code/hostedReviews.svelte";
  import { hostedReviewKey, type HostedReview } from "$lib/code/hostedReviews";
  import { untrack } from "svelte";
  import { ChevronDown, ChevronRight, GitBranch, GitPullRequestArrow, RefreshCw, Search } from "@lucide/svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { humanizeForgeMessage, type ItemProjection } from "$lib/forge";
  import { reviewQueue, reviewQueueState, reviewRepository } from "$lib/code/reviewQueue";

  let { rail = false, onPick }: { rail?: boolean; onPick?: () => void } = $props();
  let collapsed = $state<string[]>([]);
  let query = $state("");
  let providerId = $state("");
  let filter = $state<"all" | "active" | "reviewed">("all");
  let opening = $state<string | null>(null);
  let error = $state<string | null>(null);
  const filters = [{ id: "all", label: "All" }, { id: "active", label: "Active" }, { id: "reviewed", label: "Reviewed" }] as const;
  const hostedRows = $derived(hostedReviews.reviews.filter((review) => {
    const active = review.state === "draft" || review.state === "open";
    return (filter === "all" || (filter === "active" ? active : !active)) && (!query || `${review.title} ${review.reference.repository}`.toLowerCase().includes(query.toLowerCase()));
  }));
  const sources = $derived([...new Set([...hostedReviews.providers.filter(p => p.id === hostedReviews.accountProvider || (!hostedReviews.accountProvider && hostedReviews.providers.length === 1)).map(p => p.id), ...hostedRows.map(r => r.reference.provider_id)])].map(id => ({ id, label: hostedReviews.providers.find(p => p.id === id)?.label ?? id, rows: hostedRows.filter(r => r.reference.provider_id === id) })));
  const providerLookup = $derived(query.trim().startsWith("https://") || /^[\w.-]+\/[\w.-]+$/.test(query.trim()));
  const inboxAvailable = $derived(hostedReviews.providers.some(p => p.operations.includes("inbox")));
  const rows = $derived(reviewQueue(undertakings.items, "all", query).filter(item => filter === "all" || (filter === "active" ? reviewQueueState(item) !== "reviewed" : reviewQueueState(item) === "reviewed")));
  const labels = { ready: "Ready", drafts: "In progress", reviewed: "Reviewed" };
  function toggleSource(id: string) { collapsed = collapsed.includes(id) ? collapsed.filter(value => value !== id) : [...collapsed, id]; }
  const selected = $derived.by(() => {
    const tab = shellTabs.activeTab;
    const resource = tab?.kind === "lme" ? lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId) : null;
    return resource?.kind === "code" && (resource.resource.kind === "review" || resource.resource.kind === "hosted_review") ? resource.workId : null;
  });
  $effect(() => {
    if (!rail) return;
    void chat.workshopScopeId;
    untrack(() => {
      query = ""; filter = "all"; error = null; providerId = "";
      hostedReviews.reset(chat.workshopScopeId);
      void hostedReviews.discover();
      void undertakings.refreshList();
      undertakings.startPolling();
    });
  });

  $effect(() => {
    const account = hostedReviews.connectionId;
    if (rail && inboxAvailable) untrack(() => { void account; void hostedReviews.refresh(); });
  });
  function chooseInbox(value: string) {
    if (value === "repository") { void hostedReviews.refresh(); return; }
    void hostedReviews.inbox(value as "authored" | "review_requested" | "participating");
  }
  async function openHosted(review: HostedReview) {
    await lmeWorkspace.openHostedReview(review.reference, review.title, hostedReviews.connectionId); onPick?.();
  }
  async function openProviderQuery() {
    const scope = chat.workshopScopeId;
    const input = query;
    const account = hostedReviews.connectionId;
    const review = await hostedReviews.open(input, providerId || undefined);
    if (scope !== chat.workshopScopeId || account !== hostedReviews.connectionId) return;
    if (!hostedReviews.error && query === input) query = "";
    if (review) await openHosted(review);
  }
  async function openReview(item: ItemProjection) {
    opening = item.id;
    error = null;
    try { await lmeWorkspace.openCodeReview(item.id, item.title); onPick?.(); }
    catch (cause) { error = humanizeForgeMessage(cause instanceof Error ? cause.message : String(cause)); }
    finally { opening = null; }
  }
</script>

{#if rail}
<section class="reviews-rail" aria-label="Review list">
  <header class="lme-side-rail-dock"><span>Reviews</span><button type="button" aria-label="Refresh reviews" disabled={undertakings.loading} onclick={() => { void undertakings.refreshList(); void hostedReviews.discover(true); void hostedReviews.refresh(true); }}><RefreshCw size={14} /></button></header>
  <div class="queue-controls">
    <label class="queue-search"><Search size={14} /><input type="search" aria-label="Search reviews" placeholder="Search reviews or paste a URL…" bind:value={query} onkeydown={(event) => { if (event.key === "Enter" && providerLookup) { event.preventDefault(); void openProviderQuery(); } }} /></label>
    {#if hostedReviews.providers.length}
      <div class="provider-controls">{#if hostedReviews.providers.length > 1}<select aria-label="Review provider" bind:value={providerId}><option value="">Choose provider</option>{#each hostedReviews.providers as provider}<option value={provider.id}>{provider.label}</option>{/each}</select>{/if}{#if providerLookup}<button type="button" disabled={hostedReviews.loading} onclick={() => void openProviderQuery()}>{hostedReviews.loading ? "Opening…" : "Open review or repository"}</button>{/if}</div>
    {/if}
    <nav aria-label="Review filters">{#each filters as entry}<button type="button" class:active={filter === entry.id} aria-pressed={filter === entry.id} onclick={() => filter = entry.id}>{entry.label}</button>{/each}</nav>
  </div>
  <div class="queue-scroll">
    <section class="source-section" aria-label="Forge reviews">
      <button type="button" class="source-heading" aria-expanded={!collapsed.includes("forge")} onclick={() => toggleSource("forge")}>
        {#if collapsed.includes("forge")}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}<GitBranch size={14} /><span>Forge</span><small>{rows.length}</small>
      </button>
      {#if !collapsed.includes("forge")}
        {#if error || undertakings.error}<p class="queue-error" role="alert">{error || humanizeForgeMessage(undertakings.error || "")}</p>{/if}
        {#if undertakings.loading && !undertakings.items.length}<p class="queue-empty" role="status">Loading Forge reviews…</p>
        {:else if !rows.length}<p class="queue-empty">{query || filter !== "all" ? "No matching Forge reviews." : "No prepared changes yet."}</p>
        {:else}<ul>{#each rows as item (item.id)}{@const state = reviewQueueState(item)!}<li><button type="button" title={item.title} class="queue-row" class:selected={selected === item.id} aria-current={selected === item.id ? "page" : undefined} disabled={opening === item.id} onclick={() => void openReview(item)}><span class="queue-copy"><strong>{item.title}</strong><small><span>{reviewRepository(item)} · <span class:ready={state === "ready"}>{opening === item.id ? "Opening…" : labels[state]}</span></span>{#if item.updated_at}<time datetime={item.updated_at}>{new Date(item.updated_at).toLocaleDateString(undefined, { month: "short", day: "numeric" })}</time>{/if}</small></span></button></li>{/each}</ul>{/if}
      {/if}
    </section>
    {#each sources as source (source.id)}
      <section class="source-section" aria-label={`${source.label} reviews`}>
        <button type="button" class="source-heading" aria-expanded={!collapsed.includes(source.id)} onclick={() => toggleSource(source.id)}>
          {#if collapsed.includes(source.id)}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}<GitPullRequestArrow size={14} /><span>{source.label}</span><small>{source.rows.length}</small>
        </button>
        {#if !collapsed.includes(source.id)}
          {#if hostedReviews.providers.find(p => p.id === source.id)?.operations.includes("inbox")}
            <label class="inbox-source"><span class="sr-only">{source.label} inbox</span><select aria-label={`${source.label} inbox`} value={hostedReviews.folder} onchange={(event) => chooseInbox(event.currentTarget.value)}><option value="review_requested">Needs your review</option><option value="authored">Your pull requests</option><option value="participating">Participating</option>{#if hostedReviews.folder === "repository"}<option value="repository">{hostedReviews.repositoryName}</option>{/if}</select></label>
          {/if}
          {#if hostedReviews.error}<p class="queue-error" role="alert">{hostedReviews.error}</p>{/if}
          <ul>{#each source.rows as review (hostedReviewKey(review.reference, hostedReviews.connectionId))}<li><button type="button" class="queue-row" class:selected={selected === hostedReviewKey(review.reference, hostedReviews.connectionId)} aria-current={selected === hostedReviewKey(review.reference, hostedReviews.connectionId) ? "page" : undefined} onclick={() => void openHosted(review)}><span class="queue-copy"><strong>{review.title}</strong><small><span>{review.reference.repository.split("/").at(-1)} · #{review.reference.native_id} · {review.state}</span><time datetime={review.updated_at}>{new Date(review.updated_at).toLocaleDateString(undefined, { month: "short", day: "numeric" })}</time></small></span></button></li>{/each}</ul>
          {#if !source.rows.length}<p class="queue-empty" role={hostedReviews.loading ? "status" : undefined}>{hostedReviews.loading ? "Loading reviews…" : query || filter !== "all" ? "No matching reviews." : "Your inbox is clear."}</p>{/if}
          {#if hostedReviews.nextCursor && !query}<button type="button" class="more-reviews" disabled={hostedReviews.loading} onclick={() => void hostedReviews.more()}>Load more pull requests</button>{/if}
        {/if}
      </section>
    {/each}
    {#if !sources.length && hostedReviews.error}<p class="queue-error" role="alert">{hostedReviews.error}</p>{/if}
  </div>
</section>
{:else}
<section class="reviews-idle" aria-label="Reviews">
  <GitPullRequestArrow size={28} strokeWidth={1.4} /><h1>Choose a review</h1><p>Open a change set from the side rail to review its files and checks.</p>
  <button type="button" onclick={() => layout.openShellSidebarView("reviews")}>Browse reviews</button>
</section>
{/if}

<style>
  .reviews-rail { display:flex; flex-direction:column; height:100%; min-height:0; overflow:hidden; color:rgb(var(--theme-text)); }
  header { display:flex; align-items:center; justify-content:space-between; flex-shrink:0; font-size:.75rem; font-weight:550; color:rgb(var(--theme-text)); }
  header button { display:grid; place-items:center; padding:.35rem; border-radius:.3rem; }
  button { border:0; background:transparent; color:inherit; cursor:pointer; }
  button:disabled { opacity:.45; cursor:wait; }
  .source-section + .source-section { margin-top:1rem; }
  .source-heading { display:flex; align-items:center; gap:.4rem; width:100%; padding:.4rem .55rem; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); text-align:left; }
  .source-heading small { margin:0 0 0 auto; }
  .inbox-source { display:block; padding:0 .55rem .25rem; }
  .inbox-source select { width:100%; border:0; background:transparent; color:rgb(var(--theme-text)); font-size:.75rem; padding:.3rem 0 .6rem; }
  .provider-controls { display:flex; gap:.4rem; margin-top:.35rem; font-size:.6875rem; }
  .provider-controls button,.more-reviews { padding:.4rem .6rem; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); }
  .provider-controls select { min-width:0; max-width:100%; background:transparent; color:inherit; }
  .queue-controls { padding:.55rem .6rem .25rem; }
  .queue-search { display:flex; align-items:center; gap:.5rem; padding:.4rem .55rem; border:1px solid rgb(var(--theme-border)/.5); border-radius:.45rem; color:rgb(var(--theme-text-tertiary)); }
  input { width:100%; min-width:0; min-height:0; height:1.25rem; line-height:1.25rem; padding:0; box-shadow:none; border:0; outline:0; background:transparent; color:rgb(var(--theme-text)); font-size:.75rem; }
  nav { display:flex; gap:.15rem; margin-top:.5rem; }
  nav button { min-height:0; line-height:1.3; padding:.35rem .5rem; font-size:.6875rem; border-radius:.35rem; color:rgb(var(--theme-text-tertiary)); }
  nav button.active,button:hover { background:rgb(var(--theme-text)/.055); color:rgb(var(--theme-text)); }
  .queue-scroll { flex:1; min-height:0; overflow:auto; padding:.1rem .35rem .7rem; }
  ul { list-style:none; margin:0; padding:0; }
  .queue-row { display:flex; align-items:flex-start; gap:.55rem; width:100%; padding:.45rem .6rem; margin:.05rem 0; border-radius:.35rem; text-align:left; }
  .queue-row.selected { background:rgb(var(--theme-text)/.065); }
  .queue-copy { flex:1; min-width:0; }
  strong { display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; overflow-wrap:anywhere; font-size:.78125rem; line-height:1.35; font-weight:450; }
  small { display:flex; align-items:center; gap:.5rem; margin-top:.25rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:.6875rem; color:rgb(var(--theme-text-secondary)); }
  small > span { overflow:hidden; text-overflow:ellipsis; }
  time { flex-shrink:0; margin-left:auto; font-size:.625rem; color:rgb(var(--theme-text-tertiary)); }
  .queue-row.selected strong { font-weight:500; }
  .ready { color:rgb(var(--color-primary-300)); }
  .queue-empty,.queue-error { padding:.65rem; font-size:.75rem; line-height:1.5; color:rgb(var(--theme-text-tertiary)); }
  .queue-error { color:rgb(var(--color-amber-400)); }
  .reviews-idle { display:flex; height:100%; flex-direction:column; align-items:center; justify-content:center; text-align:center; gap:.65rem; padding:2rem; color:rgb(var(--theme-text-tertiary)); }
  .reviews-idle h1 { margin:0; font-size:1rem; font-weight:500; color:rgb(var(--theme-text-secondary)); }
  .reviews-idle p { max-width:22rem; font-size:.8125rem; line-height:1.6; }
  .reviews-idle button { padding:.45rem .65rem; border-radius:.4rem; font-size:.75rem; color:rgb(var(--theme-text-secondary)); }
</style>
