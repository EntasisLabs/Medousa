<script lang="ts">
  interface Props {
    slotId: string;
    surfaceId: string;
    flex?: string;
    selected?: boolean;
    moving?: boolean;
    onSelect?: () => void;
    onDrop?: () => void;
  }

  let {
    slotId,
    surfaceId,
    flex,
    selected = false,
    moving = false,
    onSelect,
    onDrop,
  }: Props = $props();

  function handleClick() {
    onSelect?.();
    if (moving) onDrop?.();
  }
</script>

<button
  type="button"
  class="layout-zone-slot"
  class:layout-zone-slot-selected={selected}
  class:layout-zone-slot-drop={moving}
  style:flex={flex}
  data-slot-id={slotId}
  data-surface-id={surfaceId}
  aria-label="Empty widget zone {slotId}"
  onclick={handleClick}
>
  <span class="layout-zone-slot-label">Drop widget here</span>
</button>

<style>
  .layout-zone-slot {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    min-height: 4rem;
    min-width: 0;
    margin: 0;
    padding: 0.75rem;
    border: 1px dashed color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    border-radius: 0.75rem;
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
    color: rgb(var(--color-surface-300));
    font-size: 0.75rem;
    cursor: pointer;
  }

  .layout-zone-slot-selected {
    border-color: rgb(var(--color-primary-400));
    box-shadow: 0 0 0 1px rgb(var(--color-primary-400) / 0.35);
  }

  .layout-zone-slot-drop {
    border-color: rgb(var(--color-success-400));
    background: color-mix(in srgb, var(--color-success-500) 12%, transparent);
  }

  .layout-zone-slot-label {
    pointer-events: none;
  }
</style>
