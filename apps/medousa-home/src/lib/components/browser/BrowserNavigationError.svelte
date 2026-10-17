<script lang="ts">
  import type { HumanBrowserStore } from "$lib/stores/humanBrowser.svelte";
  let { store }: { store: HumanBrowserStore } = $props();
</script>

{#if store.error}
  <div role="alert" class="flex items-center gap-2 px-3 py-1 text-xs text-content-secondary">
    <span class="min-w-0 flex-1 truncate" title={store.error}>Could not complete navigation: {store.error}</span>
    <button type="button" class="shrink-0 underline" onclick={() => void store.reload()}>Retry</button>
  </div>
{/if}
