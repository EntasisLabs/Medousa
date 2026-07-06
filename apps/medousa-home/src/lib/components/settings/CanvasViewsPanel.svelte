<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import CanvasEditViewForm from "$lib/components/settings/CanvasEditViewForm.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import type { SurfaceDef } from "$lib/types/environment";
  import { ChevronRight } from "@lucide/svelte";

  interface Props {
    surfaces: SurfaceDef[];
    navVisibleFor: (surfaceId: string) => boolean;
    editingSurfaceId?: string | null;
    confirmDeleteSurfaceId?: string | null;
    deleteBusy?: boolean;
    onRequestEdit?: (surfaceId: string) => void;
    onCancelEdit?: () => void;
    onRequestDelete?: (surfaceId: string) => void;
    onConfirmDelete?: (surfaceId: string, label: string) => void;
    onCancelDelete?: () => void;
  }

  let {
    surfaces,
    navVisibleFor,
    editingSurfaceId = null,
    confirmDeleteSurfaceId = null,
    deleteBusy = false,
    onRequestEdit,
    onCancelEdit,
    onRequestDelete,
    onConfirmDelete,
    onCancelDelete,
  }: Props = $props();

  function componentCount(surfaceId: string): number {
    return environment.mainComponentsForSurface(surfaceId).length;
  }

  function openView(surfaceId: string) {
    if (layout.isMobile) {
      layout.openCustomSurface(surfaceId);
      return;
    }
    layout.navigateDesktop(surfaceId, { bump: true });
  }
</script>

{#if surfaces.length === 0}
  <p class="canvas-views-empty">
    No custom views yet. Create one below, then open it and use <strong>Edit layout</strong> to add widgets.
  </p>
{:else}
  <ul class="canvas-view-list">
    {#each surfaces as surface (surface.id)}
      {@const Icon = environmentIcon(surface.icon)}
      {@const inNav = navVisibleFor(surface.id)}
      {@const widgets = componentCount(surface.id)}
      <li class="canvas-view-card">
        <button
          type="button"
          class="canvas-view-card-open"
          onclick={() => openView(surface.id)}
        >
          <span class="canvas-view-card-icon" aria-hidden="true">
            <Icon size={18} strokeWidth={1.75} />
          </span>
          <span class="canvas-view-card-copy">
            <span class="canvas-view-card-title">{surface.label}</span>
            <span class="canvas-view-card-meta">
              {widgets === 0 ? "Empty room" : `${widgets} widget${widgets === 1 ? "" : "s"}`}
              · {inNav ? "In nav" : "Hidden from nav"}
            </span>
          </span>
          <ChevronRight size={16} strokeWidth={2} class="canvas-view-card-chevron" aria-hidden="true" />
        </button>

        {#if editingSurfaceId === surface.id}
          <CanvasEditViewForm
            {surface}
            onSaved={() => onCancelEdit?.()}
            onCancel={() => onCancelEdit?.()}
          />
        {:else if confirmDeleteSurfaceId === surface.id}
          <div class="canvas-view-card-delete">
            <p class="canvas-view-card-delete-copy">Remove “{surface.label}” from your canvas?</p>
            <div class="canvas-view-card-delete-actions">
              <button
                type="button"
                class="canvas-view-card-delete-btn"
                disabled={deleteBusy}
                onclick={() => onConfirmDelete?.(surface.id, surface.label)}
              >
                {deleteBusy ? "Removing…" : "Remove"}
              </button>
              <button
                type="button"
                class="canvas-view-card-delete-cancel"
                disabled={deleteBusy}
                onclick={() => onCancelDelete?.()}
              >
                Cancel
              </button>
            </div>
          </div>
        {:else}
          <div class="canvas-view-card-actions">
            <button
              type="button"
              class="canvas-view-card-action"
              disabled={deleteBusy}
              onclick={() => onRequestEdit?.(surface.id)}
            >
              Edit
            </button>
            <button
              type="button"
              class="canvas-view-card-action canvas-view-card-action-danger"
              disabled={deleteBusy}
              onclick={() => onRequestDelete?.(surface.id)}
            >
              Remove
            </button>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .canvas-views-empty {
    margin: 0;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-400));
  }

  .canvas-views-empty strong {
    font-weight: 600;
    color: rgb(var(--color-surface-300));
  }

  .canvas-view-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.45rem;
  }

  .canvas-view-card {
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 40%, transparent);
    overflow: hidden;
  }

  .canvas-view-card-open {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    width: 100%;
    border: 0;
    padding: 0.65rem 0.7rem;
    text-align: left;
    background: transparent;
    cursor: pointer;
    transition: background 140ms ease;
  }

  .canvas-view-card-open:hover {
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .canvas-view-card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.1rem;
    height: 2.1rem;
    flex-shrink: 0;
    border-radius: 0.5rem;
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .canvas-view-card-copy {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .canvas-view-card-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .canvas-view-card-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-500));
  }

  .canvas-view-card-open :global(.canvas-view-card-chevron) {
    flex-shrink: 0;
    color: rgb(var(--color-surface-500));
  }

  .canvas-view-card-actions {
    display: flex;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .canvas-view-card-action {
    flex: 1 1 50%;
    border: 0;
    padding: 0.35rem 0.7rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-400));
    background: transparent;
    cursor: pointer;
    transition:
      color 140ms ease,
      background 140ms ease;
  }

  .canvas-view-card-action:first-child {
    border-right: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .canvas-view-card-action:hover:not(:disabled) {
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .canvas-view-card-action-danger:hover:not(:disabled) {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, var(--color-error-600) 10%, transparent);
  }

  .canvas-view-card-action:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .canvas-view-card-delete {
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
    padding: 0.55rem 0.7rem 0.65rem;
  }

  .canvas-view-card-delete-copy {
    margin: 0 0 0.45rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .canvas-view-card-delete-actions {
    display: flex;
    gap: 0.45rem;
  }

  .canvas-view-card-delete-btn {
    border: 0;
    border-radius: 999px;
    padding: 0.28rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-error-100));
    background: color-mix(in srgb, var(--color-error-600) 28%, transparent);
    cursor: pointer;
  }

  .canvas-view-card-delete-cancel {
    border: 0;
    background: transparent;
    padding: 0.28rem 0.35rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }
</style>
