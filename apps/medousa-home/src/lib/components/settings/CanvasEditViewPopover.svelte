<script lang="ts">
  import { tick } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    SURFACE_ICON_GROUPS,
    type AllowedSurfaceIcon,
    isAllowedSurfaceIcon,
  } from "$lib/utils/environmentIconCatalog";
  import type { SurfaceDef } from "$lib/types/environment";
  import { Check, Trash2 } from "@lucide/svelte";

  interface Props {
    surface: SurfaceDef;
    anchorEl: HTMLElement | null;
    onClose: () => void;
    onSaved?: () => void;
    onDeleted?: () => void;
  }

  let { surface, anchorEl, onClose, onSaved, onDeleted }: Props = $props();

  let label = $state("");
  let icon = $state<AllowedSurfaceIcon>("sparkles");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let confirmDelete = $state(false);
  let iconOpen = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let panelEl = $state<HTMLDivElement | null>(null);
  let pos = $state<{ top: number; left: number; width: number } | null>(null);

  const SelectedIcon = $derived(environmentIcon(icon));
  const dirty = $derived(
    label.trim() !== surface.label.trim() ||
      icon !== (isAllowedSurfaceIcon(surface.icon) ? surface.icon : "sparkles"),
  );

  let syncedSurfaceId = $state<string | null>(null);

  function syncFromSurface(next: SurfaceDef) {
    label = next.label;
    icon = isAllowedSurfaceIcon(next.icon) ? next.icon : "sparkles";
    error = null;
    confirmDelete = false;
    iconOpen = false;
  }

  function placePanel() {
    if (!anchorEl) return;
    const rect = anchorEl.getBoundingClientRect();
    const width = 280;
    const margin = 8;
    let left = rect.right + 10;
    if (left + width > window.innerWidth - margin) {
      left = Math.max(margin, rect.left - width - 10);
    }
    let top = rect.top - 6;
    const approxHeight = iconOpen ? 320 : 72;
    if (top + approxHeight > window.innerHeight - margin) {
      top = Math.max(margin, window.innerHeight - approxHeight - margin);
    }
    if (top < margin) top = margin;
    pos = { top, left, width };
  }

  $effect(() => {
    const nextId = surface.id;
    if (syncedSurfaceId === nextId) return;
    syncedSurfaceId = nextId;
    syncFromSurface(surface);
    void tick().then(() => {
      placePanel();
      inputEl?.focus();
      inputEl?.select();
    });
  });

  $effect(() => {
    void iconOpen;
    void anchorEl;
    placePanel();
    const onReposition = () => placePanel();
    window.addEventListener("resize", onReposition);
    window.addEventListener("scroll", onReposition, true);
    return () => {
      window.removeEventListener("resize", onReposition);
      window.removeEventListener("scroll", onReposition, true);
    };
  });

  function selectIcon(name: AllowedSurfaceIcon) {
    icon = name;
    iconOpen = false;
    void tick().then(() => inputEl?.focus());
  }

  async function save() {
    if (busy || !label.trim()) return;
    error = null;
    busy = true;
    try {
      await environment.updateCustomView({
        surfaceId: surface.id,
        label: label.trim(),
        icon,
      });
      onSaved?.();
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await environment.removeCustomSurface(surface.id);
      onDeleted?.();
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      confirmDelete = false;
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (iconOpen) {
        iconOpen = false;
        return;
      }
      if (confirmDelete) {
        confirmDelete = false;
        return;
      }
      onClose();
      return;
    }
    if (event.key === "Enter" && !iconOpen && !confirmDelete) {
      event.preventDefault();
      void save();
    }
  }

  function onScrimPointerDown(event: PointerEvent) {
    const target = event.target;
    if (target instanceof Node && panelEl?.contains(target)) return;
    onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<BodyPortal>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="canvas-edit-popover-scrim"
    role="presentation"
    onpointerdown={onScrimPointerDown}
  ></div>

  {#if pos}
    <div
      bind:this={panelEl}
      class="canvas-edit-popover"
      style:top="{pos.top}px"
      style:left="{pos.left}px"
      style:width="{pos.width}px"
      role="dialog"
      aria-label="Edit custom view"
      onpointerdown={(event) => event.stopPropagation()}
    >
      {#if confirmDelete}
        <p class="canvas-edit-popover-copy">Delete this view permanently?</p>
        <div class="canvas-edit-popover-row">
          <button
            type="button"
            class="canvas-edit-popover-ghost"
            disabled={busy}
            onclick={() => (confirmDelete = false)}
          >
            Keep
          </button>
          <button
            type="button"
            class="canvas-edit-popover-danger"
            disabled={busy}
            onclick={() => void remove()}
          >
            Delete
          </button>
        </div>
      {:else}
        <div class="canvas-edit-popover-row">
          <button
            type="button"
            class="canvas-edit-popover-icon"
            class:canvas-edit-popover-icon-open={iconOpen}
            title="Change icon"
            aria-label="Change nav icon"
            aria-expanded={iconOpen}
            disabled={busy}
            onclick={() => {
              iconOpen = !iconOpen;
            }}
          >
            <SelectedIcon size={16} strokeWidth={1.85} aria-hidden="true" />
          </button>

          <input
            bind:this={inputEl}
            class="canvas-edit-popover-input"
            type="text"
            bind:value={label}
            placeholder="View name"
            disabled={busy}
            aria-label="View name"
          />

          <button
            type="button"
            class="canvas-edit-popover-save"
            title="Save"
            aria-label="Save"
            disabled={busy || !label.trim() || !dirty}
            onclick={() => void save()}
          >
            <Check size={14} strokeWidth={2.5} aria-hidden="true" />
          </button>

          <button
            type="button"
            class="canvas-edit-popover-trash"
            title="Delete view"
            aria-label="Delete view"
            disabled={busy}
            onclick={() => {
              iconOpen = false;
              confirmDelete = true;
            }}
          >
            <Trash2 size={13} strokeWidth={2} aria-hidden="true" />
          </button>
        </div>

        {#if iconOpen}
          <div class="canvas-edit-popover-icons" role="listbox" aria-label="Choose nav icon">
            {#each Object.entries(SURFACE_ICON_GROUPS) as [group, icons] (group)}
              <p class="canvas-edit-popover-icon-group">{group}</p>
              {#each icons as name (name)}
                {@const Icon = environmentIcon(name)}
                <button
                  type="button"
                  class="canvas-edit-popover-icon-option"
                  class:canvas-edit-popover-icon-option-active={icon === name}
                  role="option"
                  aria-selected={icon === name}
                  title={name}
                  disabled={busy}
                  onclick={() => selectIcon(name)}
                >
                  <Icon size={15} strokeWidth={1.75} aria-hidden="true" />
                </button>
              {/each}
            {/each}
          </div>
        {/if}

        {#if error}
          <p class="canvas-edit-popover-error">{error}</p>
        {/if}
      {/if}
    </div>
  {/if}
</BodyPortal>

<style>
  .canvas-edit-popover-scrim {
    position: fixed;
    inset: 0;
    z-index: 160;
  }

  .canvas-edit-popover {
    position: fixed;
    z-index: 165;
    display: grid;
    gap: 0.4rem;
    padding: 0.4rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.4);
    background: rgb(var(--color-surface-900) / 0.97);
    box-shadow:
      0 0 0 1px rgb(0 0 0 / 0.2),
      0 12px 32px rgb(0 0 0 / 0.45);
  }

  .canvas-edit-popover-row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .canvas-edit-popover-icon {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    border-radius: 0.45rem;
    color: rgb(var(--color-surface-100));
    background: rgb(var(--color-surface-50) / 0.05);
    cursor: pointer;
  }

  .canvas-edit-popover-icon:hover:not(:disabled),
  .canvas-edit-popover-icon-open {
    border-color: rgb(var(--color-primary-400) / 0.45);
    background: rgb(var(--color-primary-500) / 0.16);
  }

  .canvas-edit-popover-input {
    min-width: 0;
    flex: 1;
    height: 2rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    border-radius: 0.45rem;
    padding: 0 0.55rem;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-surface-950) / 0.45);
  }

  .canvas-edit-popover-input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-400) / 0.55);
  }

  .canvas-edit-popover-save,
  .canvas-edit-popover-trash,
  .canvas-edit-popover-ghost,
  .canvas-edit-popover-danger {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    cursor: pointer;
  }

  .canvas-edit-popover-save {
    width: 2rem;
    height: 2rem;
    border-radius: 0.45rem;
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600) / 0.5);
  }

  .canvas-edit-popover-save:hover:not(:disabled) {
    background: rgb(var(--color-primary-600) / 0.7);
  }

  .canvas-edit-popover-save:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .canvas-edit-popover-trash {
    width: 1.85rem;
    height: 2rem;
    border-radius: 0.45rem;
    color: rgb(var(--color-surface-400));
    background: transparent;
  }

  .canvas-edit-popover-trash:hover:not(:disabled) {
    color: rgb(var(--color-error-300));
    background: rgb(var(--color-error-500) / 0.12);
  }

  .canvas-edit-popover-icons {
    display: grid;
    max-height: 12.5rem;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 0.2rem;
    overflow: auto;
    padding: 0.2rem 0.1rem 0.1rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
  }

  .canvas-edit-popover-icon-group {
    grid-column: 1 / -1;
    margin: 0.3rem 0 0.05rem;
    font-size: 0.6rem;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .canvas-edit-popover-icon-group:first-child {
    margin-top: 0.1rem;
  }

  .canvas-edit-popover-icon-option {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    aspect-ratio: 1;
    border: 0;
    border-radius: 0.35rem;
    color: rgb(var(--color-surface-300));
    background: transparent;
    cursor: pointer;
  }

  .canvas-edit-popover-icon-option:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.55);
  }

  .canvas-edit-popover-icon-option-active {
    color: rgb(var(--color-primary-200));
    background: rgb(var(--color-primary-500) / 0.22);
  }

  .canvas-edit-popover-copy {
    margin: 0.1rem 0.15rem 0;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-200));
  }

  .canvas-edit-popover-ghost {
    flex: 1;
    height: 1.85rem;
    border-radius: 0.4rem;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-200));
    background: rgb(var(--color-surface-50) / 0.06);
  }

  .canvas-edit-popover-danger {
    flex: 1;
    height: 1.85rem;
    border-radius: 0.4rem;
    font-size: 0.75rem;
    font-weight: 650;
    color: rgb(var(--color-error-100));
    background: rgb(var(--color-error-500) / 0.3);
  }

  .canvas-edit-popover-error {
    margin: 0 0.15rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-error-300));
  }
</style>
