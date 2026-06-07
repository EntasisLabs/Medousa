<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCard } from "$lib/types/workspace";
  import { columnLabel } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";

  import { findBlockedGroupForCard } from "$lib/utils/groupWork";

  interface Props {
    card: WorkCard;
    selected: boolean;
    onSelect: (id: string) => void;
    groupCount?: number;
  }

  let { card, selected, onSelect, groupCount = 1 }: Props = $props();

  const blockedGroup = $derived(
    card.column === "blocked" && groupCount > 1
      ? findBlockedGroupForCard(workspace.cards, card.id)
      : null,
  );

  const wrappingUp = $derived(card.column === "wrapping_up");
  const draggable = $derived(workspace.isCancellable(card));

  function handleDragStart(event: DragEvent) {
    if (!draggable || !event.dataTransfer) return;
    event.dataTransfer.setData("application/x-medousa-card", card.id);
    event.dataTransfer.effectAllowed = "move";
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(card.id);
    }
  }
</script>

<div
  role="button"
  tabindex="0"
  class="card w-full p-3 text-left transition hover:brightness-110 {selected
    ? 'ring-2 ring-primary-500 bg-primary-500/10'
    : ''} {wrappingUp ? 'animate-pulse border-warning-500/60' : ''} {draggable
    ? 'cursor-grab active:cursor-grabbing'
    : 'cursor-pointer'}"
  draggable={draggable}
  ondragstart={handleDragStart}
  onclick={() => onSelect(card.id)}
  onkeydown={handleKeydown}
>
  <div class="flex items-center justify-between gap-2">
    <span class="badge variant-soft-surface text-xs capitalize">
      {columnLabel(card.column)}
    </span>
    <span class="truncate text-xs text-surface-400">
      {formatStatusLabel(card.status_label)}
    </span>
  </div>
  <div class="mt-2 flex items-start justify-between gap-2">
    <p class="line-clamp-3 text-sm font-medium leading-snug">
      {formatCardTitle(card)}
    </p>
    {#if groupCount > 1}
      <span class="badge variant-soft-warning shrink-0 text-[10px]">×{groupCount}</span>
    {/if}
  </div>
  {#if blockedGroup && blockedGroup.cards.length > 1}
    <div class="mt-3 flex flex-wrap gap-1.5">
      <button
        type="button"
        class="btn btn-sm variant-soft-primary"
        onclick={(event) => {
          event.stopPropagation();
          void workspace.retryBlockedGroup(blockedGroup);
        }}
      >
        Retry ×{blockedGroup.cards.length}
      </button>
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={(event) => {
          event.stopPropagation();
          void workspace.dismissBlockedGroup(blockedGroup);
        }}
      >
        Dismiss
      </button>
    </div>
  {:else if draggable}
    <p class="mt-2 text-[10px] uppercase tracking-wide text-surface-500">
      drag to cancel
    </p>
  {/if}
</div>
