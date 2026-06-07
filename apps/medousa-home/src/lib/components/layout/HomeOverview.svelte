<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import { columnLabel } from "$lib/types/workspace";
  import { KANBAN_COLUMNS } from "$lib/types/work";

  interface Props {
    onOpenWork: () => void;
  }

  let { onOpenWork }: Props = $props();

  const totalActive = $derived(workspace.activeCards().length);
</script>

<section class="flex flex-1 flex-col items-center justify-center p-8">
  <div class="w-full max-w-lg text-center">
    <h2 class="text-lg font-semibold text-surface-100">Medousa Home</h2>
    <p class="mt-2 text-sm text-surface-400">
      {totalActive} active card{totalActive === 1 ? "" : "s"} on the board right now.
    </p>

    <div class="mt-6 grid grid-cols-2 gap-3 sm:grid-cols-3">
      {#each KANBAN_COLUMNS.filter((c) => c !== "done") as column (column)}
        <div class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-3">
          <p class="text-xs capitalize text-surface-400">{columnLabel(column)}</p>
          <p class="mt-1 text-2xl font-semibold text-surface-100">
            {workspace.columnCounts[column] ??
              workspace.cards.filter((card) => card.column === column).length}
          </p>
        </div>
      {/each}
    </div>

    <button
      type="button"
      class="btn variant-filled-primary mt-6"
      onclick={onOpenWork}
    >
      Open work board
    </button>
  </div>
</section>
