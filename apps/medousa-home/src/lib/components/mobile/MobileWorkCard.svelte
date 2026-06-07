<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCard } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
  import { columnAccentBorder } from "$lib/utils/kanban";

  interface Props {
    card: WorkCard;
    onSelect: () => void;
    swipeCancel?: boolean;
    onCanceled?: (cardId: string, message: string) => void;
  }

  let { card, onSelect, swipeCancel = false, onCanceled }: Props = $props();

  let offsetX = $state(0);
  let startX = 0;
  let swiped = false;
  let busy = false;

  const CANCEL_WIDTH = 88;
  const CANCEL_THRESHOLD = -72;

  function onTouchStart(event: TouchEvent) {
    if (!swipeCancel || busy) return;
    startX = event.touches[0].clientX;
    swiped = false;
  }

  function onTouchMove(event: TouchEvent) {
    if (!swipeCancel || busy) return;
    const delta = event.touches[0].clientX - startX;
    if (delta < -8) swiped = true;
    offsetX = Math.max(delta, -CANCEL_WIDTH);
    if (swiped && offsetX < 0) event.preventDefault();
  }

  async function onTouchEnd() {
    if (!swipeCancel || busy) {
      offsetX = 0;
      return;
    }
    if (offsetX <= CANCEL_THRESHOLD) {
      busy = true;
      try {
        const response = await workspace.cancelCard(card.id);
        onCanceled?.(card.id, response.message);
      } catch (err) {
        onCanceled?.(
          card.id,
          err instanceof Error ? err.message : String(err),
        );
      } finally {
        busy = false;
      }
    }
    offsetX = 0;
    swiped = false;
  }

  function handleClick() {
    if (swiped) return;
    onSelect();
  }
</script>

<div class="mobile-swipe-row relative overflow-hidden rounded-container-token">
  {#if swipeCancel}
    <div class="mobile-swipe-cancel-bg" aria-hidden="true">Cancel</div>
  {/if}
  <button
    type="button"
    class="workshop-kanban-card relative w-full text-left transition-transform {columnAccentBorder(
      card.column,
    )} {card.column === 'wrapping_up' ? 'animate-pulse' : ''}"
    style:transform={offsetX ? `translateX(${offsetX}px)` : undefined}
    ontouchstart={onTouchStart}
    ontouchmove={onTouchMove}
    ontouchend={onTouchEnd}
    onclick={handleClick}
  >
    <p class="text-sm font-medium text-surface-100">{formatCardTitle(card)}</p>
    <p class="workshop-faint mt-0.5 capitalize">
      {formatStatusLabel(card.status_label)}
    </p>
  </button>
</div>
