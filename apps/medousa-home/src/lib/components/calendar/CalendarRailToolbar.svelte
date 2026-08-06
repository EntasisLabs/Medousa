<script lang="ts">
  import { CalendarDays, ListTodo, Plus, RefreshCw, Search, X } from "@lucide/svelte";
  import { calendar } from "$lib/stores/calendar.svelte";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { tick } from "svelte";

  interface Props {
    onAction?: () => void;
    /** popover = New · Refresh · Search; rail-row = New · Search. */
    variant?: "popover" | "rail-row";
  }

  let { onAction, variant = "popover" }: Props = $props();

  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let createMenuOpen = $state(false);

  const searching = $derived(calendar.railQuery.trim().length > 0);

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  function closeCreateMenu() {
    createMenuOpen = false;
  }

  function toggleCreateMenu(event: MouseEvent) {
    event.stopPropagation();
    createMenuOpen = !createMenuOpen;
  }

  function createEvent() {
    closeCreateMenu();
    calendar.openCreate(calendar.selectedDay);
    onAction?.();
  }

  function createReminder() {
    closeCreateMenu();
    calendar.openCreateReminder(calendar.selectedDay);
    onAction?.();
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeCreateMenu();
    }
  }

  async function openSearch() {
    closeCreateMenu();
    await ensureRailPopoverOpen();
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    calendar.setRailQuery("");
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }
</script>

<svelte:window onclick={closeCreateMenu} />

{#if searchExpanded}
  <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
    <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
    <input
      bind:this={searchInputEl}
      class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
      type="search"
      placeholder="Search events…"
      value={calendar.railQuery}
      oninput={(event) =>
        calendar.setRailQuery((event.currentTarget as HTMLInputElement).value)}
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
  <div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>

  <div class="relative shrink-0">
    <button
      type="button"
      class="vault-dock-icon-btn"
      title="New"
      aria-label="New"
      aria-haspopup="menu"
      aria-expanded={createMenuOpen}
      onclick={toggleCreateMenu}
    >
      <Plus size={16} strokeWidth={1.75} />
    </button>
    {#if createMenuOpen}
      <div
        class="absolute top-full right-0 z-30 mt-1 min-w-[11rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
        role="menu"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onkeydown={handleMenuKeydown}
      >
        <button
          type="button"
          role="menuitem"
          class="vault-menu-item"
          onclick={createEvent}
        >
          <CalendarDays size={14} strokeWidth={2} />
          New event
        </button>
        <button
          type="button"
          role="menuitem"
          class="vault-menu-item"
          onclick={createReminder}
        >
          <ListTodo size={14} strokeWidth={2} />
          New reminder
        </button>
      </div>
    {/if}
  </div>

  {#if variant === "popover"}
    <div class="lme-dock-chrome-secondary shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn"
        title="Refresh"
        aria-label="Refresh calendar"
        disabled={calendar.loading}
        onclick={() => void calendar.refresh()}
      >
        <RefreshCw
          size={15}
          strokeWidth={1.75}
          class={calendar.loading ? "animate-spin" : ""}
        />
      </button>
    </div>
  {/if}

  <button
    type="button"
    class="vault-dock-icon-btn {searching ? 'vault-dock-icon-btn-active' : ''}"
    title="Search events"
    aria-label="Search events"
    onclick={() => void openSearch()}
  >
    <Search size={15} strokeWidth={1.75} />
  </button>
{/if}
