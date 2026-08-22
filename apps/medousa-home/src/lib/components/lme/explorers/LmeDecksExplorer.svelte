<script lang="ts">
  import { LayoutTemplate, RefreshCw, Search, X } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import ArtifactLibraryList from "$lib/components/artifacts/ArtifactLibraryList.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";

  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const query = $derived(artifacts.searchQuery);
  const searching = $derived(query.trim().length > 0);
  const refreshing = $derived(artifacts.loading);

  onMount(() => {
    void artifacts.refresh();
  });

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  async function openSearch() {
    await ensureRailPopoverOpen();
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    artifacts.setSearchQuery("");
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }
</script>

<aside class="lme-decks-explorer flex h-full min-h-0 w-full flex-col" aria-label="Artifacts">
  {#if artifacts.error}
    <p class="shrink-0 px-3 py-2 text-sm text-content-error">{artifacts.error}</p>
  {/if}

  <header class="lme-side-rail-dock" use:portLmeDock>
    {#if searchExpanded}
      <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
        <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
          type="search"
          placeholder="Search artifacts…"
          value={query}
          oninput={(event) => artifacts.setSearchQuery(event.currentTarget.value)}
          onkeydown={handleSearchKeydown}
        />
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Close search"
          title="Close search"
          onclick={closeSearch}
        >
          <X size={14} strokeWidth={1.75} />
        </button>
      </div>
    {:else}
      <div class="lme-artifacts-dock-identity min-w-0 flex-1">
        <LayoutTemplate size={14} strokeWidth={1.7} aria-hidden="true" />
        <span>Artifacts</span>
        {#if artifacts.artifacts.length > 0}
          <span class="lme-artifacts-dock-count">{artifacts.artifacts.length}</span>
        {/if}
      </div>

      <div class="lme-dock-chrome-secondary shrink-0">
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Refresh artifacts"
          title="Refresh"
          disabled={refreshing}
          onclick={() => void artifacts.refresh()}
        >
          <RefreshCw size={15} strokeWidth={1.75} class={refreshing ? "animate-spin" : ""} />
        </button>
      </div>

      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="Search artifacts"
        title="Search"
        onclick={() => void openSearch()}
      >
        <Search size={15} strokeWidth={1.75} />
      </button>
    {/if}
  </header>
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if artifacts.loading && artifacts.artifacts.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else}
      <ArtifactLibraryList
        artifacts={artifacts.filteredArtifacts}
        selectedArtifactId={artifacts.selectedArtifactId}
        emptyLabel={searching ? "No artifacts match." : "No artifacts yet."}
        onSelect={(artifactId) => {
          const entry = artifacts.artifacts.find((row) => row.artifact_id === artifactId);
          lmeWorkspace.openDeck(artifactId, entry?.label);
        }}
      />
    {/if}
  </div>

</aside>

<style>
  .lme-artifacts-dock-identity {
    display: flex;
    align-items: center;
    gap: 0.38rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: -0.008em;
  }

  .lme-artifacts-dock-identity :global(svg) {
    flex: 0 0 auto;
    color: rgb(var(--theme-text-tertiary));
  }

  .lme-artifacts-dock-count {
    color: rgb(var(--theme-text-quiet));
    font-size: 0.59375rem;
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }
</style>
