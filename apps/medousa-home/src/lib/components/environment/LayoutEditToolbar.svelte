<script lang="ts">
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import { isMobileLayoutEdit } from "$lib/utils/layoutEditGestures";

  interface Props {
    surfaceId: string;
  }

  let { surfaceId }: Props = $props();

  const mobile = $derived(isMobileLayoutEdit());
</script>

{#if layoutEdit.isEditingSurface(surfaceId)}
  <div class="layout-edit-toolbar" role="toolbar" aria-label="Layout edit tools">
    <div class="layout-edit-toolbar-group">
      <button type="button" class="layout-edit-btn" onclick={() => layoutEdit.addZone()}>
        Add zone
      </button>
      <button
        type="button"
        class="layout-edit-btn"
        disabled={!layoutEdit.selectedId || layoutEdit.selectedId.startsWith("zone-")}
        onclick={() => layoutEdit.splitHorizontal()}
      >
        Split ↔
      </button>
      <button
        type="button"
        class="layout-edit-btn"
        disabled={!layoutEdit.selectedId || layoutEdit.selectedId.startsWith("zone-")}
        onclick={() => layoutEdit.splitVertical()}
      >
        Split ↕
      </button>
      <button type="button" class="layout-edit-btn" onclick={() => layoutEdit.resetLayout()}>
        Reset
      </button>
    </div>
    <div class="layout-edit-toolbar-group">
      {#if mobile}
        <span class="layout-edit-hint">Tap select · Long-press pick up · Double-tap drop</span>
      {/if}
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
    gap: 0.5rem;
    padding: 0.45rem 0.65rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 88%, transparent);
  }

  .layout-edit-toolbar-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }

  .layout-edit-btn {
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    border-radius: 0.45rem;
    padding: 0.3rem 0.55rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
    cursor: pointer;
  }

  .layout-edit-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .layout-edit-btn-primary {
    border-color: color-mix(in srgb, var(--color-primary-500) 55%, transparent);
    color: rgb(var(--color-primary-100));
  }

  .layout-edit-hint {
    font-size: 0.625rem;
    color: rgb(var(--color-surface-400));
    margin-right: 0.35rem;
  }

  .layout-edit-error {
    width: 100%;
    margin: 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-error-300));
  }
</style>
