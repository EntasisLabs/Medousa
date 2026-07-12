<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { presetDescription, presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { Check, Focus, PanelsTopLeft, Settings2 } from "@lucide/svelte";

  interface Props {
    variant?: "settings" | "rail";
  }

  let { variant = "settings" }: Props = $props();

  const presets = $derived(environment.spec?.layoutPresets ?? []);
  const activePreset = $derived(
    presets.find((preset) => preset.active) ??
      presets.find((preset) => preset.id === environment.spec?.activePresetId) ??
      null,
  );
  const showRail = $derived(variant === "rail" && presets.length > 1);
  const showSettings = $derived(variant === "settings" && presets.length > 0);
  const show = $derived(showRail || showSettings);
  const activeLabel = $derived(
    presetDisplayLabel(activePreset?.id ?? "default", activePreset?.label),
  );

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

  function openCanvasSettings() {
    open = false;
    settingsNav.openSection("canvas");
    layout.navigateDesktop("settings", { bump: true });
  }

  function presetIcon(presetId: string) {
    return presetId === "focus" ? Focus : PanelsTopLeft;
  }
</script>

{#if show}
  {#if variant === "rail"}
    <button
      type="button"
      class="workshop-rail-btn workshop-rail-btn-tier-utility relative mt-2 {open
        ? 'workshop-rail-workshop-btn-open'
        : ''} {activePreset?.id === 'focus' ? 'workshop-rail-btn-active' : ''}"
      title="Canvas layout — {activeLabel}"
      aria-label="Canvas layout — {activeLabel}"
      aria-haspopup="menu"
      aria-expanded={open}
      disabled={busy}
      onclick={() => (open = !open)}
    >
      <PanelsTopLeft size={16} strokeWidth={1.5} aria-hidden="true" />
    </button>

    {#if open}
      <div
        class="mobile-sheet-backdrop workshop-rail-sheet-backdrop"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) open = false;
        }}
      >
        <div class="workshop-rail-sheet workshop-switcher-menu" role="menu" aria-label="Canvas layout">
          <header class="workshop-switcher-header">
            <div class="min-w-0">
              <h2 class="workshop-switcher-title">Canvas layout</h2>
              <p class="workshop-switcher-subtitle">Choose which destinations appear in the rail</p>
            </div>
          </header>

          <div class="workshop-switcher-list">
            {#each presets as preset (preset.id)}
              {@const isActive = preset.id === activePreset?.id}
              {@const Icon = presetIcon(preset.id)}
              <button
                type="button"
                role="menuitemradio"
                aria-checked={isActive}
                class="workshop-switcher-row {isActive ? 'workshop-switcher-row-active' : ''}"
                disabled={busy}
                onclick={() => void selectPreset(preset.id)}
              >
                <span class="workshop-switcher-avatar" aria-hidden="true">
                  <Icon size={16} strokeWidth={1.75} />
                </span>
                <span class="workshop-switcher-row-body">
                  <span class="workshop-switcher-row-name">
                    {presetDisplayLabel(preset.id, preset.label)}
                  </span>
                  <span class="workshop-switcher-row-meta">{presetDescription(preset.id)}</span>
                </span>
                {#if isActive}
                  <Check size={16} strokeWidth={2.5} class="workshop-switcher-row-check" aria-hidden="true" />
                {/if}
              </button>
            {/each}
          </div>

          <div class="workshop-switcher-footer">
            <div class="workshop-switcher-divider" aria-hidden="true"></div>
            <button
              type="button"
              role="menuitem"
              class="workshop-switcher-action"
              onclick={openCanvasSettings}
            >
              <span class="workshop-switcher-action-icon" aria-hidden="true">
                <Settings2 size={14} strokeWidth={2} />
              </span>
              Canvas settings — layouts & views
            </button>
          </div>
        </div>
      </div>
    {/if}
  {:else}
    <div class="env-preset-segment" role="group" aria-label="Layout preset">
      {#each presets as preset (preset.id)}
        <button
          type="button"
          class="env-preset-segment-btn"
          class:env-preset-segment-btn-active={preset.id === activePreset?.id}
          aria-pressed={preset.id === activePreset?.id}
          disabled={busy}
          onclick={() => void selectPreset(preset.id)}
        >
          {presetDisplayLabel(preset.id, preset.label)}
        </button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .env-preset-segment {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: stretch;
    max-width: 100%;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    overflow: hidden;
  }

  .env-preset-segment-btn {
    border: 0;
    border-right: 1px solid color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    padding: 0.38rem 0.85rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-300));
    background: transparent;
    cursor: pointer;
    transition:
      background 140ms ease,
      color 140ms ease;
  }

  .env-preset-segment-btn:last-child {
    border-right: 0;
  }

  .env-preset-segment-btn:hover:not(:disabled):not(.env-preset-segment-btn-active) {
    background: color-mix(in srgb, var(--color-surface-800) 75%, transparent);
    color: rgb(var(--color-surface-100));
  }

  .env-preset-segment-btn-active {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-primary-600) 35%, transparent);
  }

  .env-preset-segment-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
