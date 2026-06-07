<script lang="ts">
  import { columnLabel, type WorkCard } from "$lib/types/workspace";

  interface Props {
    cards: WorkCard[];
    selectedId: string | null;
    onSelect: (id: string) => void | Promise<void>;
  }

  let { cards, selectedId, onSelect }: Props = $props();

  function columnTone(column: string): string {
    switch (column) {
      case "in_flight":
        return "variant-filled-primary";
      case "wrapping_up":
        return "variant-filled-warning";
      case "blocked":
        return "variant-filled-error";
      case "done":
        return "variant-soft-surface";
      default:
        return "variant-soft-surface";
    }
  }
</script>

<section
  class="flex h-28 shrink-0 items-stretch gap-2 overflow-x-auto border-t border-surface-500/20 bg-surface-900/80 px-3 py-2"
  aria-label="Active work"
>
  {#if cards.length === 0}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      No active work — cards appear when jobs or turns run
    </div>
  {:else}
    {#each cards as card (card.id)}
      <button
        type="button"
        class="card min-w-[220px] max-w-[280px] shrink-0 p-3 text-left transition {selectedId ===
        card.id
          ? 'ring-2 ring-primary-500'
          : 'hover:brightness-110'} {card.column === 'wrapping_up'
          ? 'animate-pulse border-warning-500/50'
          : ''}"
        onclick={() => onSelect(card.id)}
      >
        <div class="flex items-center justify-between gap-2">
          <span class="badge {columnTone(card.column)} text-xs capitalize">
            {columnLabel(card.column)}
          </span>
          <span class="text-xs text-surface-400 truncate">{card.status_label}</span>
        </div>
        <p class="mt-2 line-clamp-2 text-sm font-medium">{card.title}</p>
      </button>
    {/each}
  {/if}
</section>
