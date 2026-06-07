<script lang="ts">
  import MobileActivityFeed from "$lib/components/mobile/MobileActivityFeed.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    onOpenNote: (path: string) => void;
  }

  let { onOpenNote: _onOpenNote }: Props = $props();

  $effect(() => {
    if (layout.activitySheetOpen) {
      void workspace.prefetchActivityCardDetails();
    }
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
    <div class="mobile-sheet mobile-sheet-tall" role="dialog" aria-label="Activity">
      <header class="mobile-sheet-header mobile-activity-sheet-header">
        <div class="min-w-0">
          <h2 class="text-sm font-semibold text-surface-50">Activity</h2>
          <p class="workshop-faint mt-0.5 text-xs">What Medousa has been doing</p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface shrink-0"
          onclick={() => layout.setActivitySheetOpen(false)}
        >
          Done
        </button>
      </header>
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto">
        <MobileActivityFeed events={workspace.feed} error={workspace.streamError} />
      </div>
    </div>
  </div>
{/if}
