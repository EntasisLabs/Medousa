<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCard } from "$lib/types/workspace";
  import { columnLabel } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";

  interface Props {
    card: WorkCard;
    selected: boolean;
    onSelect: (id: string) => void;
  }

  let { card, selected, onSelect }: Props = $props();

  const wrappingUp = $derived(card.column === "wrapping_up");
  const draggable = $derived(workspace.isCancellable(card));

  function handleDragStart(event: DragEvent) {
    if (!draggable || !event.dataTransfer) return;
    event.dataTransfer.setData("application/x-medousa-card", card.id);
    event.dataTransfer.effectAllowed = "move";
  }
</script>

<button
  type="button"
  class="card w-full p-3 text-left transition hover:brightness-110 {selected
    ? 'ring-2 ring-primary-500 bg-primary-500/10'
    : ''} {wrappingUp ? 'animate-pulse border-warning-500/60' : ''} {draggable
    ? 'cursor-grab active:cursor-grabbing'
    : ''}"
  draggable={draggable}
  ondragstart={handleDragStart}
  onclick={() => onSelect(card.id)}
>
  <div class="flex items-center justify-between gap-2">
    <span class="badge variant-soft-surface text-xs capitalize">
      {columnLabel(card.column)}
    </span>
    <span class="truncate text-xs text-surface-400">
      {formatStatusLabel(card.status_label)}
    </span>
  </div>
  <p class="mt-2 line-clamp-3 text-sm font-medium leading-snug">
    {formatCardTitle(card)}
  </p>
  {#if draggable}
    <p class="mt-2 text-[10px] uppercase tracking-wide text-surface-500">
      drag to cancel
    </p>
  {/if}
</button>
