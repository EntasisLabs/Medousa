<script lang="ts">
  import { RefreshCw, Search, X } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import ArtifactLibraryList from "$lib/components/artifacts/ArtifactLibraryList.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

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

<aside class="lme-decks-explorer flex h-full min-h-0 w-full flex-col" aria-label="Presentations">
  {#if artifacts.error}
    <p class="shrink-0 px-3 py-2 text-sm text-error-400">{artifacts.error}</p>
  {/if}

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if artifacts.loading && artifacts.artifacts.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else}
      <ArtifactLibraryList
        artifacts={artifacts.filteredArtifacts}
        selectedArtifactId={artifacts.selectedArtifactId}
        sessionTitle={(sessionId) => artifacts.sessionTitle(sessionId)}
        onSelect={(artifactId) => {
          const entry = artifacts.artifacts.find((row) => row.artifact_id === artifactId);
          lmeWorkspace.openDeck(artifactId, entry?.label);
        }}
        onOpenChat={(artifact) => {
          void artifact;
          onOpenChat();
        }}
      />
    {/if}
  </div>

  <footer class="lme-side-rail-dock" use:portLmeDock>
    {#if searchExpanded}
      <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
        <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
          type="search"
          placeholder="Search presentations…"
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
      <div class="lme-dock-chrome-secondary shrink-0">
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Refresh presentations"
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
        aria-label="Search presentations"
        title="Search"
        onclick={() => void openSearch()}
      >
        <Search size={15} strokeWidth={1.75} />
      </button>
    {/if}
  </footer>
</aside>
