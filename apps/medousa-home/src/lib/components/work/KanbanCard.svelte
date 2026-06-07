<script lang="ts">
  import type { WorkCard } from "$lib/types/workspace";
  import { columnLabel } from "$lib/types/workspace";

  interface Props {
    card: WorkCard;
    selected: boolean;
    onSelect: (id: string) => void;
  }

  let { card, selected, onSelect }: Props = $props();

  const wrappingUp = $derived(card.column === "wrapping_up");
</script>

<button
  type="button"
  class="card w-full p-3 text-left transition hover:brightness-110 {selected
    ? 'ring-2 ring-primary-500'
    : ''} {wrappingUp ? 'animate-pulse border-warning-500/60' : ''}"
  onclick={() => onSelect(card.id)}
>
  <div class="flex items-center justify-between gap-2">
    <span class="badge variant-soft-surface text-xs capitalize">
      {columnLabel(card.column)}
    </span>
    <span class="truncate text-xs text-surface-400">{card.status_label}</span>
  </div>
  <p class="mt-2 line-clamp-3 text-sm font-medium leading-snug">{card.title}</p>
</button>
