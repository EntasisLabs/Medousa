<script lang="ts">
  import { onMount } from "svelte";
  import { MessageCircle, Plus, Search, Users, X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { tick } from "svelte";

  interface Props {
    /** Fired after creating a session. */
    onCreated?: () => void;
    variant?: "popover" | "rail-row";
  }

  let { onCreated, variant = "popover" }: Props = $props();

  onMount(() => {
    void sharedMode.load();
  });

  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  let query = $state(chat.sessionListQuery);

  const searching = $derived(query.trim().length > 0);

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  $effect(() => {
    if (!searchExpanded) query = chat.sessionListQuery;
  });

  $effect(() => {
    const needle = query;
    chat.sessionListQuery = needle;
  });

  async function openSearch() {
    await ensureRailPopoverOpen();
    query = chat.sessionListQuery;
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    query = "";
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }

  async function createSession() {
    await chat.newSession();
    onCreated?.();
  }

  async function createSharedRoom() {
    try {
      await chat.newSharedRoom();
      onCreated?.();
    } catch (err) {
      console.error(err);
    }
  }
</script>

{#if searchExpanded}
  <div class="lme-dock-search-expand flex-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search titles…"
      bind:value={query}
      onkeydown={handleSearchKeydown}
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
  {#if variant === "rail-row"}
    <div class="session-rail-title min-w-0 flex-1">
      <MessageCircle size={13} strokeWidth={1.75} aria-hidden="true" />
      <span>Chats</span>
    </div>
  {:else}
    <div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>
  {/if}

  <button
    type="button"
    class="vault-dock-icon-btn"
    title="New chat"
    aria-label="New chat"
    onclick={() => void createSession()}
  >
    <Plus size={16} strokeWidth={1.75} />
  </button>
  {#if variant === "popover" && sharedMode.isShared}
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="New shared room"
      aria-label="New shared room"
      onclick={() => void createSharedRoom()}
    >
      <Users size={15} strokeWidth={1.75} />
    </button>
  {/if}
  <button
    type="button"
    class="vault-dock-icon-btn {searching ? 'vault-dock-icon-btn-active' : ''}"
    title="Search sessions"
    aria-label="Search sessions"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}
