<script lang="ts">
  import { columnLabel, type WorkCard } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";

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
  class="flex h-28 shrink-0 items-stretch gap-2 overflow-x-auto border-t border-surface-500/50 bg-surface-800/90 px-3 py-2 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]"
  aria-label="Active work"
>
  {#if cards.length === 0}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      No in-motion work — backlog, in flight, and wrapping up appear here
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
          <span class="truncate text-xs text-surface-400">
            {formatStatusLabel(card.status_label)}
          </span>
        </div>
        <p class="mt-2 line-clamp-2 text-sm font-medium">
          {formatCardTitle(card)}
        </p>
      </button>
    {/each}
  {/if}
</section>
