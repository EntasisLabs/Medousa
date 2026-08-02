<script lang="ts">
  import { onMount } from "svelte";
  import { Plus, Search, Users, X } from "@lucide/svelte";
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

  let searchOpen = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  let query = $state(chat.sessionListQuery);

  $effect(() => {
    if (!searchOpen) query = chat.sessionListQuery;
  });

  $effect(() => {
    const needle = query;
    chat.sessionListQuery = needle;
  });

  async function openSearch() {
    await ensureRailPopoverOpen();
    query = chat.sessionListQuery;
    searchOpen = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchOpen = false;
    query = "";
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
    class="vault-dock-icon-btn"
    title="Search sessions"
    aria-label="Search sessions"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}
