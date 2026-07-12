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

{#if cards.length === 0}
  <p class="workshop-faint px-1 py-2 text-center text-[11px]">No in-motion work</p>
{:else}
  <ul class="work-motion-peek-list" aria-label="In-motion work">
    {#each cards as card (card.id)}
      <li>
        <button
          type="button"
          class="work-motion-peek-card {columnAccentBorder(card.column)} {selectedId ===
          card.id
            ? 'work-motion-peek-card--selected'
            : ''}"
          onclick={() => onSelect(card.id)}
        >
          <p class="workshop-faint capitalize">{columnLabel(card.column)}</p>
          <p class="mt-0.5 line-clamp-2 text-left text-sm leading-snug text-surface-100">
            {formatCardTitle(card)}
          </p>
          <p class="mt-1 truncate font-mono text-[10px] text-surface-500">
            {formatStatusLabel(card.status_label)}
          </p>
        </button>
      </li>
    {/each}
  </ul>
{/if}
