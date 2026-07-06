<script lang="ts">
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import { isMobileLayoutEdit } from "$lib/utils/layoutEditGestures";
  import { Plus, Trash2 } from "@lucide/svelte";

  interface Props {
    surfaceId: string;
  }

  let { surfaceId }: Props = $props();

  const mobile = $derived(isMobileLayoutEdit());
  const canMerge = $derived(layoutEdit.canMergeSelected());
  const canRemove = $derived(layoutEdit.canRemoveSelected());
</script>

{#if layoutEdit.isEditingSurface(surfaceId)}
  <div class="layout-edit-toolbar" role="toolbar" aria-label="Edit layout">
    <div class="layout-edit-toolbar-top">
      <p class="layout-edit-title">Edit layout</p>
      <div class="layout-edit-toolbar-commit">
        <button type="button" class="layout-edit-text-btn" onclick={() => layoutEdit.cancel()}>
          Cancel
        </button>
        <button
          type="button"
          class="layout-edit-done-btn"
          disabled={layoutEdit.saving}
          onclick={() => void layoutEdit.save()}
        >
          {layoutEdit.saving ? "Saving…" : "Done"}
        </button>
      </div>
    </div>

    <div class="layout-edit-toolbar-tools">
      <div class="layout-edit-segment" role="group" aria-label="Pane layout">
        <button
          type="button"
          class="layout-edit-segment-btn"
          title="Split selected pane side by side"
          onclick={() => layoutEdit.splitSelected("horizontal")}
        >
          Split
        </button>
        <button
          type="button"
          class="layout-edit-segment-btn"
          title="Stack selected pane vertically"
          onclick={() => layoutEdit.splitSelected("vertical")}
        >
          Stack
        </button>
        <button
          type="button"
          class="layout-edit-segment-btn"
          disabled={!canMerge}
          title={canMerge ? "Merge selected pane with its sibling" : "Select a subdivided pane to merge"}
          onclick={() => layoutEdit.mergeSelected()}
        >
          Merge
        </button>
      </div>

      <div class="layout-edit-secondary-actions">
        <button
          type="button"
          class="layout-edit-secondary-btn"
          onclick={() => layoutEdit.openWidgetPicker()}
        >
          <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
          Add widget
        </button>
        <button
          type="button"
          class="layout-edit-secondary-btn layout-edit-secondary-btn-danger"
          disabled={!canRemove}
          title={canRemove ? "Remove widget from selected pane" : "Select a pane with a widget to remove"}
          onclick={() => layoutEdit.removeSelectedWidget()}
        >
          <Trash2 size={13} strokeWidth={2} aria-hidden="true" />
          Remove
        </button>
      </div>
    </div>

    <p class="layout-edit-hint">
      {#if mobile}
        Tap a pane to select it. Long-press a widget, then tap a destination.
      {:else}
        Select a pane, then split or stack. Drag a widget by its handle to move it.
      {/if}
    </p>

    {#if layoutEdit.error}
      <p class="layout-edit-error">{layoutEdit.error}</p>
    {/if}
  </div>
{/if}

<style>
  .layout-edit-toolbar {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    padding: 0.65rem 0.85rem 0.6rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 94%, transparent);
    backdrop-filter: blur(8px);
  }

  .layout-edit-toolbar-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .layout-edit-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-100));
  }

  .layout-edit-toolbar-commit {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .layout-edit-text-btn {
    border: 0;
    background: transparent;
    padding: 0.28rem 0.35rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
    transition: color 140ms ease;
  }

  .layout-edit-text-btn:hover {
    color: rgb(var(--color-surface-100));
  }

  .layout-edit-done-btn {
    border: 0;
    border-radius: 999px;
    padding: 0.34rem 0.85rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600));
    cursor: pointer;
    transition: background 140ms ease;
  }

  .layout-edit-done-btn:hover:not(:disabled) {
    background: rgb(var(--color-primary-500));
  }

  .layout-edit-done-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .layout-edit-toolbar-tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.65rem;
  }

  .layout-edit-segment {
    display: inline-flex;
    align-items: stretch;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    overflow: hidden;
  }

  .layout-edit-segment-btn {
    border: 0;
    border-right: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    padding: 0.32rem 0.72rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    background: transparent;
    cursor: pointer;
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .layout-edit-segment-btn:last-child {
    border-right: 0;
  }

  .layout-edit-segment-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    color: rgb(var(--color-surface-50));
  }

  .layout-edit-segment-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .layout-edit-secondary-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .layout-edit-secondary-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    border-radius: 999px;
    padding: 0.3rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    background: transparent;
    cursor: pointer;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }

  .layout-edit-secondary-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 55%, transparent);
    color: rgb(var(--color-surface-50));
  }

  .layout-edit-secondary-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .layout-edit-secondary-btn-danger:hover:not(:disabled) {
    color: rgb(var(--color-error-200));
    border-color: color-mix(in srgb, var(--color-error-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-error-600) 12%, transparent);
  }

  .layout-edit-hint {
    margin: 0;
    font-size: 0.6875rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
  }

  .layout-edit-error {
    margin: 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-error-300));
  }
</style>
