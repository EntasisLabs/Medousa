<script lang="ts">
  import { columnLabel, type WorkCard } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
  import { columnAccentBorder } from "$lib/utils/kanban";

  interface Props {
    cards: WorkCard[];
    selectedId: string | null;
    onSelect: (id: string) => void | Promise<void>;
  }

  let { cards, selectedId, onSelect }: Props = $props();
</script>

<section
  class="flex h-24 shrink-0 items-stretch gap-1.5 overflow-x-auto border-t border-surface-500/50 bg-surface-800/90 px-3 py-1.5"
  aria-label="Active work"
>
  {#if cards.length === 0}
    <div class="workshop-faint flex flex-1 items-center justify-center">
      No in-motion work
    </div>
  {:else}
    {#each cards as card (card.id)}
      <button
        type="button"
        class="workshop-kanban-card min-w-[200px] max-w-[240px] shrink-0 {columnAccentBorder(
          card.column,
        )} {selectedId === card.id
          ? 'ring-1 ring-inset ring-primary-500/40'
          : ''} {card.column === 'wrapping_up' ? 'animate-pulse' : ''}"
        onclick={() => onSelect(card.id)}
      >
        <p class="workshop-faint capitalize">{columnLabel(card.column)}</p>
        <p class="mt-0.5 line-clamp-2 text-sm leading-snug text-surface-100">
          {formatCardTitle(card)}
        </p>
        <p class="mt-1 truncate font-mono text-[10px] text-surface-500">
          {formatStatusLabel(card.status_label)}
        </p>
      </button>
    {/each}
  {/if}
</section>
