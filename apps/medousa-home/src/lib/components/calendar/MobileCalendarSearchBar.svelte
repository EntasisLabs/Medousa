<script lang="ts">
  import { Search, X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { calendar } from "$lib/stores/calendar.svelte";

  let inputEl: HTMLInputElement | undefined = $state();

  onMount(() => {
    queueMicrotask(() => inputEl?.focus());
  });

  function close() {
    calendar.closeMobileSearch({ clear: true });
  }
</script>

<div class="mobile-calendar-search" role="search">
  <Search size={17} strokeWidth={1.75} aria-hidden="true" />
  <input
    bind:this={inputEl}
    type="search"
    value={calendar.railQuery}
    placeholder="Search events and reminders"
    aria-label="Search events and reminders"
    oninput={(event) => calendar.setRailQuery(event.currentTarget.value)}
  />
  <button type="button" aria-label="Close search" onclick={close}>
    <X size={17} strokeWidth={1.75} />
  </button>
</div>

<style>
  .mobile-calendar-search {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 2.5rem;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.75rem;
    padding: 0.45rem 0.75rem 0.55rem 1rem;
    border-bottom: 1px solid rgb(var(--shell-border) / 0.5);
    color: rgb(var(--shell-muted));
    background: rgb(var(--color-surface-950));
  }

  .mobile-calendar-search input {
    min-width: 0;
    height: 2.75rem;
    border: 0;
    background: transparent;
    padding: 0 !important;
    font-size: 1rem;
    line-height: 1.35;
    color: rgb(var(--theme-text));
    outline: none;
    box-shadow: none;
  }

  .mobile-calendar-search input::placeholder {
    color: rgb(var(--theme-placeholder));
  }

  .mobile-calendar-search button {
    display: inline-flex;
    width: 2.5rem;
    height: 2.5rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    color: rgb(var(--shell-icon));
  }

  .mobile-calendar-search button:active {
    background: rgb(var(--color-surface-800) / 0.7);
  }
</style>
