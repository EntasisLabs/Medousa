<script lang="ts">
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import { isMobileLayoutEdit } from "$lib/utils/layoutEditGestures";

  interface Props {
    surfaceId: string;
  }

  let { surfaceId }: Props = $props();

  const mobile = $derived(isMobileLayoutEdit());
  const canMerge = $derived(layoutEdit.canMergeSelected());
  const canRemove = $derived(layoutEdit.canRemoveSelected());
</script>

{#if layoutEdit.isEditingSurface(surfaceId)}
  <div class="layout-edit-toolbar" role="toolbar" aria-label="Arrange widgets">
    <div class="layout-edit-toolbar-main">
      <p class="layout-edit-title">Arrange widgets</p>
      <div class="layout-tiling-actions" role="group" aria-label="Selected pane">
        <button
          type="button"
          class="layout-tiling-btn"
          title="Split selected pane side by side"
          onclick={() => layoutEdit.splitSelected("horizontal")}
        >
          Split
        </button>
        <button
          type="button"
          class="layout-tiling-btn"
          title="Stack selected pane vertically"
          onclick={() => layoutEdit.splitSelected("vertical")}
        >
          Stack
        </button>
        <button
          type="button"
          class="layout-tiling-btn"
          disabled={!canMerge}
          title={canMerge ? "Merge selected pane with its sibling" : "Select a subdivided pane to merge"}
          onclick={() => layoutEdit.mergeSelected()}
        >
          Merge
        </button>
        <button
          type="button"
          class="layout-tiling-btn layout-tiling-btn-danger"
          disabled={!canRemove}
          title={canRemove ? "Remove widget from selected pane" : "Select a pane with a widget to remove"}
          onclick={() => layoutEdit.removeSelectedWidget()}
        >
          Remove
        </button>
        <button
          type="button"
          class="layout-tiling-btn layout-tiling-btn-accent"
          onclick={() => layoutEdit.openWidgetPicker()}
        >
          Add widget
        </button>
      </div>
      {#if mobile}
        <p class="layout-edit-hint">Tap a pane to select it. Long-press a widget, then tap a destination.</p>
      {:else}
        <p class="layout-edit-hint">Select a pane, then Split or Stack. Drag by the handle to swap.</p>
      {/if}
    </div>

    <div class="layout-edit-toolbar-commit">
      <button type="button" class="layout-edit-btn" onclick={() => layoutEdit.cancel()}>
        Cancel
      </button>
      <button
        type="button"
        class="layout-edit-btn layout-edit-btn-primary"
        disabled={layoutEdit.saving}
        onclick={() => void layoutEdit.save()}
      >
        {layoutEdit.saving ? "Saving…" : "Done"}
      </button>
    </div>

    {#if layoutEdit.error}
      <p class="layout-edit-error">{layoutEdit.error}</p>
    {/if}
  </div>
{/if}

<style>
  .layout-edit-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.55rem;
    padding: 0.65rem 0.85rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 92%, transparent);
    backdrop-filter: blur(8px);
  }

  .layout-edit-toolbar-main {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 0.75rem;
  }

  .layout-edit-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .layout-edit-hint {
    margin: 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .layout-tiling-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .layout-tiling-btn {
    border-radius: 999px;
    padding: 0.28rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-300));
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    border: 1px solid transparent;
    cursor: pointer;
  }

  .layout-tiling-btn-accent {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-primary-700) 55%, transparent);
    border-color: color-mix(in srgb, var(--color-primary-500) 45%, transparent);
  }

  .layout-tiling-btn-danger {
    color: rgb(var(--color-error-200));
    border-color: color-mix(in srgb, var(--color-error-500) 35%, transparent);
  }

  .layout-tiling-btn-danger:not(:disabled):hover {
    background: color-mix(in srgb, var(--color-error-600) 22%, transparent);
  }

  .layout-tiling-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .layout-edit-toolbar-commit {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .layout-edit-btn {
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    border-radius: 999px;
    padding: 0.32rem 0.7rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .layout-edit-btn-primary {
    border-color: rgb(var(--color-primary-500));
    background: rgb(var(--color-primary-600));
    color: rgb(var(--color-surface-50));
    font-weight: 600;
  }

  .layout-edit-error {
    width: 100%;
    margin: 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-error-300));
  }
</style>
