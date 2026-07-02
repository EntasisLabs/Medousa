<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const presets = $derived(environment.spec?.layoutPresets ?? []);
  const activePreset = $derived(
    presets.find((preset) => preset.active) ??
      presets.find((preset) => preset.id === environment.spec?.activePresetId) ??
      null,
  );
  const show = $derived(presets.length > 1);

  let open = $state(false);
  let busy = $state(false);

  async function selectPreset(presetId: string) {
    if (busy || presetId === activePreset?.id) {
      open = false;
      return;
    }
    busy = true;
    try {
      await environment.activatePreset(presetId);
      open = false;
    } finally {
      busy = false;
    }
  }
</script>

{#if show}
  <div class="env-preset-switcher" class:env-preset-switcher-compact={compact}>
    <button
      type="button"
      class="env-preset-switcher-btn"
      aria-haspopup="listbox"
      aria-expanded={open}
      disabled={busy}
      onclick={() => (open = !open)}
    >
      <span class="env-preset-switcher-label">
        {activePreset?.label ?? "Layout"}
      </span>
      <ChevronDown size={14} strokeWidth={2} />
    </button>
    {#if open}
      <div class="env-preset-switcher-menu" role="listbox">
        {#each presets as preset (preset.id)}
          <button
            type="button"
            role="option"
            aria-selected={preset.id === activePreset?.id}
            class="env-preset-switcher-item"
            class:env-preset-switcher-item-active={preset.id === activePreset?.id}
            onclick={() => void selectPreset(preset.id)}
          >
            {preset.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .env-preset-switcher {
    position: relative;
    padding: 0 0.5rem 0.5rem;
  }

  .env-preset-switcher-compact {
    padding: 0;
  }

  .env-preset-switcher-btn {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 0.35rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 60%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
  }

  .env-preset-switcher-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .env-preset-switcher-menu {
    position: absolute;
    left: 0.5rem;
    right: 0.5rem;
    top: calc(100% - 0.25rem);
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 70%, transparent);
    background: rgb(var(--color-surface-900));
    padding: 0.25rem;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .env-preset-switcher-item {
    border-radius: 0.375rem;
    padding: 0.35rem 0.5rem;
    text-align: left;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-300));
  }

  .env-preset-switcher-item:hover {
    background: color-mix(in srgb, var(--color-surface-700) 50%, transparent);
  }

  .env-preset-switcher-item-active {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-primary-500) 18%, transparent);
  }
</style>
