<script lang="ts">
  import MobileActivityFeed from "$lib/components/mobile/MobileActivityFeed.svelte";
  import { haptic } from "$lib/haptics";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    onOpenNote: (path: string) => void;
  }

  let { onOpenNote: _onOpenNote }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    layout.setActivitySheetOpen(false);
  }

  $effect(() => {
    if (layout.activitySheetOpen) {
      workspace.scheduleActivityCardPrefetch();
    }
  });

  $effect(() => {
    if (!layout.activitySheetOpen || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
</script>

{#if layout.activitySheetOpen}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) layout.setActivitySheetOpen(false);
    }}
  >
    <div bind:this={sheetEl} class="mobile-sheet mobile-sheet-tall" role="dialog" aria-label="Activity">
      <header bind:this={headerEl} class="mobile-sheet-header mobile-activity-sheet-header scripts-workbench-sheet-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex w-full items-start justify-between gap-2">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Activity</h2>
            <p class="workshop-faint mt-0.5 text-xs">What Medousa has been doing</p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface shrink-0"
            onclick={dismiss}
          >
            Done
          </button>
        </div>
      </header>
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
        <MobileActivityFeed events={workspace.feed} error={workspace.streamError} />
      </div>
    </div>
  </div>
{/if}
