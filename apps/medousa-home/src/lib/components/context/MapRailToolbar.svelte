<script lang="ts">
  import { Search, X } from "@lucide/svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { tick } from "svelte";

  interface Props {
    variant?: "popover" | "rail-row";
  }

  let { variant: _variant = "popover" }: Props = $props();

  let searchOpen = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const query = $derived(contextShell.search);

  async function openSearch() {
    await ensureRailPopoverOpen();
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    contextShell.search = "";
  }
</script>

{#if searchOpen}
  <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search map…"
      value={query}
      oninput={(event) => {
        contextShell.search = (event.currentTarget as HTMLInputElement).value;
      }}
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
  <div class="min-w-0 flex-1" aria-hidden="true"></div>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Search map"
    aria-label="Search map"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}
