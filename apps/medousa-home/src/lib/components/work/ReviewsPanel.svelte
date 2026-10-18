<script lang="ts">
  import { untrack } from "svelte";
  import { GitPullRequestArrow, RefreshCw, Search } from "@lucide/svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { humanizeForgeMessage, type ItemProjection } from "$lib/forge";
  import { reviewQueue, reviewQueueState, reviewRepository, type ReviewQueueFilter } from "$lib/code/reviewQueue";

  let { rail = false, onPick }: { rail?: boolean; onPick?: () => void } = $props();
  let query = $state("");
  let filter = $state<ReviewQueueFilter>("all");
  let opening = $state<string | null>(null);
  let error = $state<string | null>(null);
  const filters = [{ id: "all", label: "All" }, { id: "ready", label: "Ready" }, { id: "drafts", label: "Drafts" }, { id: "reviewed", label: "Reviewed" }] as const;
  const rows = $derived(reviewQueue(undertakings.items, filter, query));
  const labels = { ready: "Ready", drafts: "In progress", reviewed: "Reviewed" };
  const selected = $derived.by(() => {
    const tab = shellTabs.activeTab;
    const resource = tab?.kind === "lme" ? lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId) : null;
    return resource?.kind === "code" && resource.resource.kind === "review" ? resource.workId : null;
  });
  $effect(() => {
    if (!rail) return;
    void chat.workshopScopeId;
    untrack(() => {
      query = ""; filter = "all"; error = null;
      void undertakings.refreshList();
      undertakings.startPolling();
    });
  });

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
  <header class="lme-side-rail-dock"><span>Reviews</span><button type="button" aria-label="Refresh reviews" disabled={undertakings.loading} onclick={() => void undertakings.refreshList()}><RefreshCw size={14} /></button></header>
  <div class="queue-controls">
    <label class="queue-search"><Search size={14} /><input type="search" aria-label="Search reviews" placeholder="Search reviews…" bind:value={query} /></label>
    <nav aria-label="Review filters">{#each filters as entry}<button type="button" class:active={filter === entry.id} aria-pressed={filter === entry.id} onclick={() => filter = entry.id}>{entry.label}</button>{/each}</nav>
  </div>
  {#if error || undertakings.error}<p class="queue-error" role="alert">{error || humanizeForgeMessage(undertakings.error || "")}</p>{/if}
  <div class="queue-scroll">
    {#if undertakings.loading && !undertakings.items.length}<p class="queue-empty" role="status">Loading reviews…</p>
    {:else if !rows.length}<p class="queue-empty">{query || filter !== "all" ? "No matching reviews." : "No reviews yet. Prepared changes will appear here."}</p>
    {:else}<ul>{#each rows as item (item.id)}{@const state = reviewQueueState(item)!}<li><button type="button" title={item.title} class="queue-row" class:selected={selected === item.id} aria-current={selected === item.id ? "page" : undefined} disabled={opening === item.id} onclick={() => void openReview(item)}><span class="queue-copy"><strong>{item.title}</strong><small><span class="queue-meta">{reviewRepository(item)} · <span class:ready={state === "ready"}>{opening === item.id ? "Opening…" : labels[state]}</span></span>{#if item.updated_at}<time datetime={item.updated_at}>{new Date(item.updated_at).toLocaleDateString(undefined, { month: "short", day: "numeric" })}</time>{/if}</small></span></button></li>{/each}</ul>{/if}
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
  .queue-empty,.queue-error { padding:1rem .65rem; font-size:.75rem; line-height:1.5; color:rgb(var(--theme-text-tertiary)); }
  .queue-error { color:rgb(var(--color-amber-400)); }
  .reviews-idle { display:flex; height:100%; flex-direction:column; align-items:center; justify-content:center; text-align:center; gap:.65rem; padding:2rem; color:rgb(var(--theme-text-tertiary)); }
  .reviews-idle h1 { margin:0; font-size:1rem; font-weight:500; color:rgb(var(--theme-text-secondary)); }
  .reviews-idle p { max-width:22rem; font-size:.8125rem; line-height:1.6; }
  .reviews-idle button { padding:.45rem .65rem; border-radius:.4rem; font-size:.75rem; color:rgb(var(--theme-text-secondary)); }
</style>
