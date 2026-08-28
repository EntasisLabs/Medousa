<script lang="ts">
  import AskComposer from "$lib/components/work/AskComposer.svelte";
  import { haptic } from "$lib/haptics";
  import { layout } from "$lib/runtime/layout.svelte";
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
    <div bind:this={sheetEl} class="mobile-sheet mobile-sheet-medium" role="dialog" aria-label="New ask">
      <header bind:this={headerEl} class="mobile-sheet-stack-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row">
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
        <AskComposer
          sheet={true}
          autofocus={true}
          onQueued={() => layout.setAskSheetOpen(false)}
        />
      </div>
    </div>
  </div>
{/if}
