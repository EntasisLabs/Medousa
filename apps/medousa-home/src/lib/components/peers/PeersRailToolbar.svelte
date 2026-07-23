<script lang="ts">
  import { Plus, Search, X } from "@lucide/svelte";
  import { peersShell } from "$lib/stores/peersShell.svelte";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { tick } from "svelte";

  let searchOpen = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  async function openSearch() {
    await ensureRailPopoverOpen();
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    peersShell.peopleQuery = "";
  }
</script>

{#if searchOpen && peersShell.showPeopleSearch}
  <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search people…"
      bind:value={peersShell.peopleQuery}
    />
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="Close search"
      aria-label="Close search"
      onclick={closeSearch}
    >
      <X size={14} strokeWidth={1.75} />
    </button>
  </div>
{:else}
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Add peer"
    aria-label="Add peer"
    onclick={() => peersShell.requestAddPeer()}
  >
    <Plus size={15} strokeWidth={1.75} />
  </button>
  {#if peersShell.showPeopleSearch}
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="Search people"
      aria-label="Search people"
      onclick={() => void openSearch()}
    >
      <Search size={15} strokeWidth={1.75} />
    </button>
  {/if}
{/if}
