<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    SURFACE_ICON_GROUPS,
    type AllowedSurfaceIcon,
  } from "$lib/utils/environmentIconCatalog";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    icon: AllowedSurfaceIcon;
    disabled?: boolean;
    onChange?: (icon: AllowedSurfaceIcon) => void;
  }

  let { icon, disabled = false, onChange }: Props = $props();

  let open = $state(false);
  let btnEl = $state<HTMLButtonElement | null>(null);
  let gridPos = $state<{
    top: number;
    left: number;
    width: number;
    maxHeight: number;
  } | null>(null);
  const SelectedIcon = $derived(environmentIcon(icon));

  function updateGridPos() {
    if (!btnEl) return;
    const rect = btnEl.getBoundingClientRect();
    const width = Math.max(rect.width, 240);
    const maxHeight = Math.min(14 * 16, window.innerHeight - 24);
    const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    let top = rect.bottom + 6;
    if (top + Math.min(maxHeight, 180) > window.innerHeight - 8) {
      top = Math.max(8, rect.top - maxHeight - 6);
    }
    gridPos = { top, left, width, maxHeight };
  }

  function toggleOpen() {
    if (disabled) return;
    if (open) {
      close();
      return;
    }
    open = true;
    updateGridPos();
  }

  function close() {
    open = false;
    gridPos = null;
  }

  function selectIcon(name: AllowedSurfaceIcon) {
    onChange?.(name);
    close();
  }

  $effect(() => {
    if (!open) return;
    updateGridPos();
    const reposition = () => updateGridPos();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("scroll", reposition, true);
    window.addEventListener("resize", reposition);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", reposition, true);
      window.removeEventListener("resize", reposition);
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="canvas-icon-picker">
  <button
    bind:this={btnEl}
    type="button"
    class="canvas-icon-picker-btn"
    aria-expanded={open}
    aria-haspopup="listbox"
    {disabled}
    onclick={toggleOpen}
  >
    <SelectedIcon size={16} strokeWidth={1.75} aria-hidden="true" />
    <span>{icon}</span>
    <ChevronDown size={14} aria-hidden="true" />
  </button>
</div>

{#if open && gridPos}
  <BodyPortal>
    <button
      type="button"
      class="canvas-icon-picker-backdrop"
      aria-label="Close icon picker"
      tabindex="-1"
      onclick={close}
    ></button>
    <div
      class="canvas-icon-grid"
      role="listbox"
      aria-label="Choose nav icon"
      style:top="{gridPos.top}px"
      style:left="{gridPos.left}px"
      style:width="{gridPos.width}px"
      style:max-height="{gridPos.maxHeight}px"
    >
      {#each Object.entries(SURFACE_ICON_GROUPS) as [group, icons] (group)}
        <p class="canvas-icon-group-label">{group}</p>
        {#each icons as name (name)}
          {@const Icon = environmentIcon(name)}
          <button
            type="button"
            role="option"
            aria-selected={icon === name}
            class="canvas-icon-option"
            class:canvas-icon-option-active={icon === name}
            title={name}
            onclick={() => selectIcon(name)}
          >
            <Icon size={16} strokeWidth={1.75} />
          </button>
        {/each}
      {/each}
    </div>
  </BodyPortal>
{/if}

<style>
  .canvas-icon-picker {
    position: relative;
    width: 100%;
  }

  .canvas-icon-picker-btn {
    display: inline-flex;
    width: 100%;
    align-items: center;
    gap: 0.4rem;
    border-radius: 0.45rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-600)) / 0.55);
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-label, var(--color-surface-100)));
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.6);
    cursor: pointer;
  }

  .canvas-icon-picker-btn span {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .canvas-icon-picker-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  :global(.canvas-icon-picker-backdrop) {
    position: fixed;
    inset: 0;
    z-index: 80;
    border: 0;
    padding: 0;
    background: transparent;
    cursor: default;
  }

  :global(.canvas-icon-grid) {
    position: fixed;
    z-index: 81;
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(2rem, 1fr));
    gap: 0.25rem;
    padding: 0.5rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-600)) / 0.55);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.4);
  }

  :global(.canvas-icon-group-label) {
    grid-column: 1 / -1;
    margin: 0.3rem 0 0.05rem;
    font-size: 0.625rem;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-surface-500));
  }

  :global(.canvas-icon-group-label:first-child) {
    margin-top: 0;
  }

  :global(.canvas-icon-option) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border-radius: 0.375rem;
    border: 0;
    color: rgb(var(--color-surface-300));
    background: transparent;
    cursor: pointer;
  }

  :global(.canvas-icon-option:hover) {
    background: rgb(var(--color-surface-700) / 0.55);
  }

  :global(.canvas-icon-option-active) {
    color: rgb(var(--color-primary-200));
    background: rgb(var(--color-primary-500) / 0.2);
  }
</style>
