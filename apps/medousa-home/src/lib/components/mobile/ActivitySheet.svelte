<script lang="ts">
  import ActivityPanel from "$lib/components/layout/ActivityPanel.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    onOpenNote: (path: string) => void;
  }

  let { onOpenNote }: Props = $props();
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
      <header class="mobile-sheet-header">
        <h2 class="text-sm font-semibold text-surface-50">Activity</h2>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => layout.setActivitySheetOpen(false)}
        >
          Done
        </button>
      </header>
      <div class="min-h-0 flex-1 overflow-y-auto">
        <ActivityPanel
          events={workspace.feed}
          error={workspace.streamError}
          notePath={vault.selectedPath}
          noteTitle={vault.title}
          wikilinksOut={vault.wikilinksOut}
          backlinks={vault.backlinks}
          cardDetail={null}
          cardError={workspace.cardDetailError}
          noteDiffChip={vault.diffChip()}
          {onOpenNote}
        />
      </div>
    </div>
  </div>
{/if}
