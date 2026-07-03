<script lang="ts">
  import LayoutWidgetTile from "$lib/components/environment/LayoutWidgetTile.svelte";
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import {
    createLayoutEditGestureState,
    isMobileLayoutEdit,
  } from "$lib/utils/layoutEditGestures";
  import {
    currentLayoutDragComponentId,
    isLayoutPointerDragging,
    startLayoutPointerDrag,
  } from "$lib/utils/layoutEditPointerDrag";
  import type { ComponentDef } from "$lib/types/environment";
  import { Plus } from "@lucide/svelte";

  interface Props {
    paneId: string;
    component: ComponentDef | null;
    selected?: boolean;
    editing?: boolean;
    sessionId: string;
    profileId?: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
    compact?: boolean;
    onSelect?: () => void;
  }

  let {
    paneId,
    component,
    selected = false,
    editing = false,
    sessionId,
    profileId = null,
    feedStateForComponent,
    compact = false,
    onSelect,
  }: Props = $props();

  const mobile = $derived(isMobileLayoutEdit());
  const gestures = createLayoutEditGestureState();
  const activeDragId = $derived(layoutEdit.movingId ?? currentLayoutDragComponentId());
  const isDragging = $derived(editing && activeDragId != null);
  const isDropTarget = $derived(editing && layoutEdit.dropTargetId === paneId);
  const isSource = $derived(editing && component != null && activeDragId === component.id);

  function handlePaneClick(event: MouseEvent) {
    if (event.defaultPrevented || isLayoutPointerDragging()) return;
    onSelect?.();
  }

  function handleHandlePointerDown(event: PointerEvent) {
    if (!component || mobile) return;
    layoutEdit.pickUp(component.id);
    startLayoutPointerDrag(
      component.id,
      {
        onHighlight: (targetPaneId) => layoutEdit.setDropTarget(targetPaneId),
        onComplete: (componentId, targetPaneId) => layoutEdit.dropOnPane(targetPaneId, componentId),
        onCancel: () => layoutEdit.finishDrag(),
      },
      event,
    );
  }

  function handlePointerDown() {
    if (!mobile || !component) return;
    gestures.handlePointerDown(component.id, "slot", {
      onSingleTap: () => onSelect?.(),
      onLongPress: (id) => layoutEdit.pickUp(id),
      onDoubleTap: () => layoutEdit.dropOnPane(paneId),
    });
  }

  function handlePointerUp() {
    if (!mobile) return;
    gestures.handlePointerUp(paneId, "slot", {
      onSingleTap: () => {
        onSelect?.();
        if (layoutEdit.movingId) layoutEdit.dropOnPane(paneId);
      },
      onLongPress: () => {},
      onDoubleTap: () => layoutEdit.dropOnPane(paneId),
    });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="layout-region-pane"
  data-layout-pane-id={paneId}
  class:layout-region-pane-compact={compact}
  class:layout-region-pane-target={isDropTarget}
  class:layout-region-pane-selected={editing && selected}
  class:layout-region-pane-empty={!component}
  class:layout-region-pane-view={!editing}
  onclick={editing ? handlePaneClick : undefined}
  onpointerdown={editing ? handlePointerDown : undefined}
  onpointerup={editing ? handlePointerUp : undefined}
>
  {#if component}
    <LayoutWidgetTile
      {component}
      {sessionId}
      {profileId}
      feedState={feedStateForComponent(component.id)}
      {editing}
      draggable={editing && !mobile}
      dragging={isSource}
      onHandlePointerDown={editing ? handleHandlePointerDown : undefined}
      onRemove={editing ? () => layoutEdit.removeWidget(paneId) : undefined}
    />
  {:else}
    <div class="layout-region-empty">
      <Plus size={compact ? 18 : 24} strokeWidth={1.75} aria-hidden="true" />
      <span>
        {#if isDragging}
          Release to place here
        {:else if selected}
          Empty pane — add a widget or drop one here
        {:else}
          Drop a widget here
        {/if}
      </span>
    </div>
  {/if}
</div>

<style>
  .layout-region-pane {
    display: flex;
    flex: 1 1 0%;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    height: 100%;
    border-radius: 0.85rem;
    transition:
      box-shadow 140ms ease,
      background 140ms ease;
    cursor: pointer;
  }

  .layout-region-pane-compact {
    min-height: 0;
  }

  .layout-region-pane-selected {
    box-shadow: 0 0 0 2px rgb(var(--color-primary-400));
  }

  .layout-region-pane-target {
    box-shadow: 0 0 0 2px rgb(var(--color-success-400));
    background: color-mix(in srgb, var(--color-success-500) 10%, transparent);
  }

  .layout-region-pane-selected.layout-region-pane-target {
    box-shadow:
      0 0 0 2px rgb(var(--color-primary-400)),
      0 0 0 4px rgb(var(--color-success-400));
  }

  .layout-region-pane-view {
    cursor: default;
    border-radius: 0;
  }

  .layout-region-pane-empty {
    border: 2px dashed color-mix(in srgb, var(--color-surface-500) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 40%, transparent);
  }

  .layout-region-empty {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 1rem;
    color: rgb(var(--color-surface-400));
    font-size: 0.8125rem;
    text-align: center;
  }
</style>
