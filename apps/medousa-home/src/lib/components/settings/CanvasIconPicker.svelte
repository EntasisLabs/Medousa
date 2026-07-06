<script lang="ts">
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    ALLOWED_SURFACE_ICONS,
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
  let gridPos = $state<{ top: number; left: number; width: number } | null>(null);
  const SelectedIcon = $derived(environmentIcon(icon));

  function updateGridPos() {
    if (!btnEl) return;
    const rect = btnEl.getBoundingClientRect();
    gridPos = {
      top: rect.bottom + 4,
      left: rect.left,
      width: Math.max(rect.width, 224),
    };
  }

  function toggleOpen() {
    if (disabled) return;
    open = !open;
    if (open) updateGridPos();
  }

  function close() {
    open = false;
  }

  function selectIcon(name: AllowedSurfaceIcon) {
    onChange?.(name);
    close();
  }

  $effect(() => {
    if (!open) return;
    updateGridPos();
    const reposition = () => updateGridPos();
    window.addEventListener("scroll", reposition, true);
    window.addEventListener("resize", reposition);
    return () => {
      window.removeEventListener("scroll", reposition, true);
      window.removeEventListener("resize", reposition);
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
    <SelectedIcon size={16} strokeWidth={1.75} />
    <span>{icon}</span>
    <ChevronDown size={14} />
  </button>
</div>

{#if open && gridPos}
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
    <p class="canvas-icon-group-label">all</p>
    {#each ALLOWED_SURFACE_ICONS as name (name)}
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
  </div>
{/if}

<style>
  .canvas-icon-picker {
    position: relative;
  }

  .canvas-icon-picker-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    cursor: pointer;
  }

  .canvas-icon-picker-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .canvas-icon-picker-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
    border: 0;
    padding: 0;
    background: transparent;
    cursor: default;
  }

  .canvas-icon-grid {
    position: fixed;
    z-index: 200;
    max-height: min(14rem, calc(100vh - 1rem));
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(2rem, 1fr));
    gap: 0.25rem;
    padding: 0.5rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 60%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .canvas-icon-group-label {
    grid-column: 1 / -1;
    margin: 0.25rem 0 0;
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-surface-500));
  }

  .canvas-icon-option {
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

  .canvas-icon-option:hover {
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .canvas-icon-option-active {
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 18%, transparent);
  }
</style>
