<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import CanvasEditViewForm from "$lib/components/settings/CanvasEditViewForm.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import type { SurfaceDef } from "$lib/types/environment";
  import { Pencil, Trash2 } from "@lucide/svelte";

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

  let navBusySurfaceId = $state<string | null>(null);
  let navError = $state<string | null>(null);

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

  async function setNavVisible(surfaceId: string, visible: boolean) {
    if (navBusySurfaceId || deleteBusy) return;
    navBusySurfaceId = surfaceId;
    navError = null;
    try {
      await environment.setSurfaceNavVisible(surfaceId, visible);
      if (!visible && !layout.isMobile && layout.desktopSurface === surfaceId) {
        const fallback =
          environment.navSurfaces().find((surface) => surface.id !== surfaceId)?.id ?? "chat";
        layout.navigateDesktop(fallback, { bump: true });
      }
    } catch (err) {
      navError = err instanceof Error ? err.message : String(err);
    } finally {
      navBusySurfaceId = null;
    }
  }
</script>

{#if surfaces.length === 0}
  <p class="canvas-views-empty">
    No custom views yet. Create one below, then open it and use <strong>Edit layout</strong> to add
    widgets.
  </p>
{:else}
  <div class="settings-toggle-list canvas-views-list">
    {#each surfaces as surface (surface.id)}
      {@const Icon = environmentIcon(surface.icon)}
      {@const inNav = navVisibleFor(surface.id)}
      {@const widgets = componentCount(surface.id)}
      {@const expanded =
        editingSurfaceId === surface.id || confirmDeleteSurfaceId === surface.id}
      <div class="canvas-views-item" class:canvas-views-item-expanded={expanded}>
        <div class="settings-toggle-row canvas-views-row">
          <button
            type="button"
            class="canvas-views-open"
            onclick={() => openView(surface.id)}
          >
            <span class="canvas-views-icon" aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="canvas-views-copy">
              <span class="canvas-views-title">{surface.label}</span>
              <span class="canvas-views-meta">
                {widgets === 0
                  ? "Empty room"
                  : `${widgets} widget${widgets === 1 ? "" : "s"}`}
              </span>
            </span>
          </button>

          <div class="canvas-views-trail">
            <label class="canvas-views-nav" title={inNav ? "Shown in nav" : "Hidden from nav"}>
              <span class="sr-only">Show {surface.label} in nav</span>
              <input
                type="checkbox"
                class="checkbox shrink-0"
                checked={inNav}
                disabled={navBusySurfaceId === surface.id || deleteBusy}
                onchange={(event) =>
                  void setNavVisible(
                    surface.id,
                    (event.currentTarget as HTMLInputElement).checked,
                  )}
              />
            </label>
            <button
              type="button"
              class="canvas-views-icon-btn"
              title="Edit view"
              aria-label="Edit {surface.label}"
              disabled={deleteBusy}
              onclick={() => onRequestEdit?.(surface.id)}
            >
              <Pencil size={14} strokeWidth={2} />
            </button>
            <button
              type="button"
              class="canvas-views-icon-btn canvas-views-icon-btn-danger"
              title="Remove view"
              aria-label="Remove {surface.label}"
              disabled={deleteBusy}
              onclick={() => onRequestDelete?.(surface.id)}
            >
              <Trash2 size={14} strokeWidth={2} />
            </button>
          </div>
        </div>

        {#if editingSurfaceId === surface.id}
          <div class="canvas-views-expand">
            <CanvasEditViewForm
              {surface}
              onSaved={() => onCancelEdit?.()}
              onCancel={() => onCancelEdit?.()}
            />
          </div>
        {:else if confirmDeleteSurfaceId === surface.id}
          <div class="canvas-views-expand canvas-views-delete">
            <p class="canvas-views-delete-copy">Remove “{surface.label}” from your canvas?</p>
            <div class="canvas-views-delete-actions">
              <button
                type="button"
                class="btn btn-sm variant-filled-error"
                disabled={deleteBusy}
                onclick={() => onConfirmDelete?.(surface.id, surface.label)}
              >
                {deleteBusy ? "Removing…" : "Remove"}
              </button>
              <button
                type="button"
                class="btn btn-sm btn-ghost"
                disabled={deleteBusy}
                onclick={() => onCancelDelete?.()}
              >
                Cancel
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

{#if navError}
  <p class="canvas-views-error">{navError}</p>
{/if}

<style>
  .canvas-views-empty {
    margin: 0;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .canvas-views-empty strong {
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-300)));
  }

  .canvas-views-list {
    overflow: hidden;
  }

  .canvas-views-item + .canvas-views-item {
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.35);
  }

  .canvas-views-row {
    gap: 0.65rem;
  }

  .canvas-views-open {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    gap: 0.55rem;
    border: 0;
    padding: 0;
    text-align: left;
    background: transparent;
    cursor: pointer;
  }

  .canvas-views-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.65rem;
    height: 1.65rem;
    flex-shrink: 0;
    border-radius: 0.4rem;
    color: rgb(var(--shell-label, var(--color-surface-300)));
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.7);
  }

  .canvas-views-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .canvas-views-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.875rem;
    font-weight: 550;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .canvas-views-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .canvas-views-trail {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.2rem;
  }

  .canvas-views-nav {
    display: inline-flex;
    align-items: center;
    padding: 0.15rem;
    cursor: pointer;
  }

  .canvas-views-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.35rem;
    padding: 0.3rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
    background: transparent;
    cursor: pointer;
    transition:
      color 120ms ease,
      background 120ms ease;
  }

  .canvas-views-icon-btn:hover:not(:disabled) {
    color: rgb(var(--shell-label, var(--color-surface-100)));
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.65);
  }

  .canvas-views-icon-btn-danger:hover:not(:disabled) {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, rgb(var(--color-error-600)) 12%, transparent);
  }

  .canvas-views-icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .canvas-views-expand {
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
    padding: 0.75rem 1rem 0.9rem;
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-900)) / 0.28);
  }

  .canvas-views-delete-copy {
    margin: 0 0 0.55rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .canvas-views-delete-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .canvas-views-error {
    margin: 0.5rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
