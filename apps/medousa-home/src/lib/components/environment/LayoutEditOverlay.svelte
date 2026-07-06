<script lang="ts">
  import type { Snippet } from "svelte";
  import CanvasWidgetPickerModal from "$lib/components/environment/CanvasWidgetPickerModal.svelte";
  import LayoutEditToolbar from "$lib/components/environment/LayoutEditToolbar.svelte";
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import { onMount } from "svelte";

  interface Props {
    surfaceId: string;
    editing?: boolean;
    dashboard?: boolean;
    children?: Snippet;
  }

  let { surfaceId, editing = false, dashboard = false, children }: Props = $props();

  const isEditing = $derived(editing || layoutEdit.isEditingSurface(surfaceId));

  function handleKeydown(event: KeyboardEvent) {
    if (!layoutEdit.isEditingSurface(surfaceId) || event.key !== "Escape") return;
    if (layoutEdit.widgetPickerOpen) {
      layoutEdit.closeWidgetPicker();
      return;
    }
    layoutEdit.cancel();
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

<div
  class="layout-edit-overlay"
  class:layout-edit-overlay-active={isEditing}
  class:layout-edit-overlay-dashboard={dashboard}
>
  {#if isEditing}
    <LayoutEditToolbar {surfaceId} />
  {/if}
  {#if children}
    {@render children()}
  {/if}
  <CanvasWidgetPickerModal
    open={isEditing && layoutEdit.widgetPickerOpen}
    {surfaceId}
    onClose={() => layoutEdit.closeWidgetPicker()}
    onAdded={(componentId) => layoutEdit.onWidgetAdded(surfaceId, componentId)}
  />
</div>

<style>
  .layout-edit-overlay {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    transition: background 180ms ease;
  }

  .layout-edit-overlay-active {
    background: color-mix(in srgb, var(--color-surface-950) 18%, transparent);
  }

  .layout-edit-overlay-active.layout-edit-overlay-dashboard :global(.layout-widget-tile) {
    border-radius: 0;
    box-shadow: none;
  }

  .layout-edit-overlay-active.layout-edit-overlay-dashboard :global(.layout-region-pane) {
    border-radius: 0;
  }

  .layout-edit-overlay-active :global(.tiling-layout-view) {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  :global(body.layout-edit-pointer-dragging) {
    cursor: grabbing;
    user-select: none;
  }

  :global(body.layout-edit-pointer-dragging iframe),
  :global(body.layout-edit-pointer-dragging .presentation-frame),
  :global(body.layout-edit-pointer-dragging .media-embed-frame),
  :global(body.layout-edit-pointer-dragging .layout-widget-tile-body) {
    pointer-events: none !important;
  }
</style>
