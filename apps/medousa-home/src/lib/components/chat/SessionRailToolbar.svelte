<script lang="ts">
  import { Plus, Search, X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { tick } from "svelte";

  interface Props {
    /** Fired after creating a session. */
    onCreated?: () => void;
  }

  let { onCreated }: Props = $props();

  let searchOpen = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  let query = $state(chat.sessionListQuery);

  $effect(() => {
    query = chat.sessionListQuery;
  });

  $effect(() => {
    const needle = query;
    chat.sessionListQuery = needle;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      searchTimer = null;
      void chat.refreshSessions({ force: true, q: needle });
    }, 300);
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });

  async function openSearch() {
    await ensureRailPopoverOpen();
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    query = "";
    void chat.refreshSessions({ force: true, q: "" });
  }

  async function createSession() {
    await chat.newSession();
    onCreated?.();
  }
</script>

{#if searchOpen}
  <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search titles…"
      bind:value={query}
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
    title="New chat"
    aria-label="New chat"
    onclick={() => void createSession()}
  >
    <Plus size={16} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Search sessions"
    aria-label="Search sessions"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}
