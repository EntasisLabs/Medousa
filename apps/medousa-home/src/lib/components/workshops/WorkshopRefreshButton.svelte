<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  let { variant = "menu", onHealthChange }: {
    variant?: "card" | "menu" | "mobile";
    onHealthChange?: () => void;
  } = $props();
</script>

<button
  type="button"
  role={variant === "card" ? undefined : "menuitem"}
  class={variant === "card" ? "workshop-refresh-card" : variant === "menu"
    ? "workshop-switcher-action" : "btn btn-sm variant-soft-primary workshop-switcher-mobile-add"}
  disabled={workshops.switching || workshops.refreshing || workshops.loading}
  aria-label="Refresh connection to {workshops.activeLabel}"
  onclick={() => void workshops.refreshConnection(onHealthChange)}
>
  <RefreshCw size={14} class={workshops.refreshing ? "animate-spin" : ""} aria-hidden="true" />
  {workshops.refreshing ? "Refreshing…" : variant === "card" ? "Refresh" : "Refresh connection"}
</button>

<style>
  .workshop-refresh-card {
    display: inline-flex; flex-shrink: 0; align-items: center; gap: 0.35rem;
    border: 0; background: transparent; padding: 0;
    font-size: 0.72rem; font-weight: 600;
    color: rgb(var(--theme-text-secondary)); cursor: pointer;
  }
  .workshop-refresh-card:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
