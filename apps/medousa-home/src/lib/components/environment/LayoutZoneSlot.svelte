<script lang="ts">
  import { Plus } from "@lucide/svelte";

  interface Props {
    slotId: string;
    surfaceId: string;
    flex?: string;
    fill?: boolean;
    selected?: boolean;
    moving?: boolean;
    dropTarget?: boolean;
    onSelect?: () => void;
    onDrop?: () => void;
    onDragOver?: () => void;
    onDragLeave?: () => void;
  }

  let {
    slotId,
    surfaceId,
    flex,
    fill = false,
    selected = false,
    moving = false,
    dropTarget = false,
    onSelect,
    onDrop,
    onDragOver,
    onDragLeave,
  }: Props = $props();

  function handleClick() {
    onSelect?.();
    if (moving) onDrop?.();
  }

  function handleDragOver(event: DragEvent) {
    if (!moving) return;
    event.preventDefault();
    onDragOver?.();
  }

  function handleDrop(event: DragEvent) {
    if (!moving) return;
    event.preventDefault();
    onDrop?.();
  }
</script>

<button
  type="button"
  class="layout-zone-slot"
  class:layout-zone-slot-fill={fill}
  class:layout-zone-slot-selected={selected}
  class:layout-zone-slot-drop={moving}
  class:layout-zone-slot-target={dropTarget}
  style:flex={flex}
  data-slot-id={slotId}
  data-surface-id={surfaceId}
  aria-label="Empty widget pane {slotId}"
  onclick={handleClick}
  ondragover={handleDragOver}
  ondragleave={onDragLeave}
  ondrop={handleDrop}
>
  <Plus size={20} strokeWidth={1.75} aria-hidden="true" />
  <span class="layout-zone-slot-label">
    {#if moving}
      Release to place widget
    {:else}
      Empty pane — drop a widget here
    {/if}
  </span>
</button>

<style>
  .layout-zone-slot {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    min-height: 5rem;
    min-width: 0;
    margin: 0;
    padding: 1rem;
    border: 2px dashed color-mix(in srgb, var(--color-surface-500) 45%, transparent);
    border-radius: 1rem;
    background: color-mix(in srgb, var(--color-surface-800) 35%, transparent);
    color: rgb(var(--color-surface-400));
    font-size: 0.8125rem;
    cursor: pointer;
    transition:
      border-color 140ms ease,
      background 140ms ease,
      box-shadow 140ms ease;
  }

  .layout-zone-slot-fill {
    flex: 1 1 0%;
    min-height: 0;
    height: 100%;
  }

  .layout-zone-slot-selected {
    border-color: rgb(var(--color-primary-400));
  }

  .layout-zone-slot-drop {
    border-color: color-mix(in srgb, var(--color-surface-500) 65%, transparent);
  }

  .layout-zone-slot-target {
    border-color: rgb(var(--color-success-400));
    background: color-mix(in srgb, var(--color-success-500) 14%, transparent);
    box-shadow: 0 0 0 1px rgb(var(--color-success-400) / 0.35);
    color: rgb(var(--color-success-200));
  }

  .layout-zone-slot-label {
    pointer-events: none;
    text-align: center;
    max-width: 14rem;
    line-height: 1.35;
  }
</style>
