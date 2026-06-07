<script lang="ts">
  import NewWorkAsk from "$lib/components/work/NewWorkAsk.svelte";
  import { layout } from "$lib/stores/layout.svelte";
</script>

{#if layout.askSheetOpen}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) layout.setAskSheetOpen(false);
    }}
  >
    <div class="mobile-sheet" role="dialog" aria-label="New ask">
      <header class="mobile-sheet-header">
        <h2 class="text-sm font-semibold text-surface-50">New ask</h2>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => layout.setAskSheetOpen(false)}
        >
          Cancel
        </button>
      </header>
      <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
        <NewWorkAsk
          visible={true}
          sheet={true}
          onQueued={() => layout.setAskSheetOpen(false)}
        />
      </div>
    </div>
  </div>
{/if}
