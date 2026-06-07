<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCard } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
  import { columnAccentBorder } from "$lib/utils/kanban";
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
  class="workshop-kanban-card {columnAccentBorder(card.column)} {selected
    ? 'bg-surface-700/60 ring-1 ring-inset ring-primary-500/40'
    : ''} {wrappingUp ? 'animate-pulse' : ''} {draggable
    ? 'cursor-grab active:cursor-grabbing'
    : 'cursor-pointer'}"
  draggable={draggable}
  ondragstart={handleDragStart}
  onclick={() => onSelect(card.id)}
  onkeydown={handleKeydown}
>
  <div class="flex items-start justify-between gap-2">
    <p class="line-clamp-2 text-sm leading-snug text-surface-100">
      {formatCardTitle(card)}
    </p>
    {#if groupCount > 1}
      <span class="shrink-0 font-mono text-[10px] tabular-nums text-surface-500">
        ×{groupCount}
      </span>
    {/if}
  </div>
  <p class="mt-1 truncate font-mono text-[10px] text-surface-500">
    {formatStatusLabel(card.status_label)}
  </p>

  {#if blockedGroup && blockedGroup.cards.length > 1}
    <div class="mt-2 flex items-center gap-3">
      <button
        type="button"
        class="workshop-text-action"
        onclick={(event) => {
          event.stopPropagation();
          void workspace.retryBlockedGroup(blockedGroup);
        }}
      >
        Retry all
      </button>
      <button
        type="button"
        class="workshop-text-action"
        onclick={(event) => {
          event.stopPropagation();
          void workspace.dismissBlockedGroup(blockedGroup);
        }}
      >
        Dismiss
      </button>
    </div>
  {:else if draggable}
    <p class="workshop-faint mt-1.5">drag to cancel</p>
  {/if}
</div>
