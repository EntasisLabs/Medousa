<script lang="ts">
  import NewWorkAsk from "$lib/components/work/NewWorkAsk.svelte";
  import { haptic } from "$lib/haptics";
  import { layout } from "$lib/stores/layout.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    layout.setAskSheetOpen(false);
  }

  $effect(() => {
    if (!layout.askSheetOpen || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
</script>

{#if layout.askSheetOpen}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) layout.setAskSheetOpen(false);
    }}
  >
    <div bind:this={sheetEl} class="mobile-sheet" role="dialog" aria-label="New ask">
      <header bind:this={headerEl} class="mobile-sheet-header scripts-workbench-sheet-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex w-full items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-surface-50">New ask</h2>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={dismiss}
          >
            Cancel
          </button>
        </div>
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
