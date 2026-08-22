<script lang="ts">
  import "$lib/styles/workshop-surfaces.postcss";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import type { ColorThemeId } from "$lib/types/colorThemes";
  import { COLOR_THEME_OPTIONS } from "$lib/types/colorThemes";
  import { presetDescription, presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { placeRailPopover, placeToolbarPopover } from "$lib/utils/railPopover";
  import {
    popBrowserPopoverOverlay,
    pushBrowserPopoverOverlay,
  } from "$lib/utils/browserPopoverOverlay";
  import {
    announceStatusPopoverOpen,
    closeOnOtherStatusPopover,
  } from "$lib/utils/statusPopoverCoordination";
  import {
    Check,
    ChevronDown,
    Focus,
    Moon,
    PanelsTopLeft,
    Pencil,
    Sun,
  } from "@lucide/svelte";
  import { onMount, tick } from "svelte";

  interface Props {
    variant?: "settings" | "rail" | "status";
    /** When rail is expanded, show a short label beside the icon. */
    expanded?: boolean;
  }

  let { variant = "settings", expanded = false }: Props = $props();

  const MENU_WIDTH = 280;

  const presets = $derived(environment.spec?.layoutPresets ?? []);
  const activePreset = $derived(
    presets.find((preset) => preset.active) ??
      presets.find((preset) => preset.id === environment.spec?.activePresetId) ??
      null,
  );
  // Status/rail stay available with a single layout so theme + light controls remain reachable.
  const showRail = $derived(variant === "rail" && presets.length > 0);
  const showStatus = $derived(variant === "status" && presets.length > 0);
  const showSettings = $derived(variant === "settings" && presets.length > 0);
  const show = $derived(showRail || showStatus || showSettings);
  const isFloatingMenu = $derived(variant === "rail" || variant === "status");
  const activeLabel = $derived(
    presetDisplayLabel(activePreset?.id ?? "default", activePreset?.label),
  );
  const activeThemeOption = $derived(
    COLOR_THEME_OPTIONS.find((option) => option.id === settings.colorTheme) ??
      COLOR_THEME_OPTIONS[0]!,
  );

  let open = $state(false);
  let themePickerOpen = $state(false);
  let busy = $state(false);
  let themeBusy = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!open) themePickerOpen = false;
  });

  $effect(() => {
    if (!open || !isFloatingMenu) return;
    void pushBrowserPopoverOverlay();
    return () => void popBrowserPopoverOverlay();
  });

  $effect(() => {
    if (!open || !isFloatingMenu || !triggerEl || !menuEl) return;
    layout.shellSidebarWidth;
    themePickerOpen;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      if (variant === "status") {
        placeToolbarPopover(triggerEl, menuEl, {
          prefer: "above",
          width: MENU_WIDTH,
          gap: 8,
          pad: 10,
          align: "start",
        });
      } else {
        placeRailPopover(triggerEl, menuEl);
      }
      frame = window.requestAnimationFrame(() => {
        if (!triggerEl || !menuEl) return;
        if (variant === "status") {
          placeToolbarPopover(triggerEl, menuEl, {
            prefer: "above",
            width: MENU_WIDTH,
            gap: 8,
            pad: 10,
            align: "start",
          });
        } else {
          placeRailPopover(triggerEl, menuEl);
        }
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
    };
  });

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

  async function pickTheme(themeId: ColorThemeId) {
    if (themeBusy || settings.colorTheme === themeId) {
      themePickerOpen = false;
      return;
    }
    themeBusy = true;
    try {
      await environment.setActiveLayoutColorTheme(themeId);
      themePickerOpen = false;
    } catch {
      settings.setColorTheme(themeId);
      themePickerOpen = false;
    } finally {
      themeBusy = false;
    }
  }

  function flipMode() {
    settings.setDarkMode(!settings.darkMode);
  }

  function openEditDestinations() {
    open = false;
    layout.startRailLayoutEditing();
  }

  function toggleMenu() {
    const next = !open;
    if (next && variant === "status") announceStatusPopoverOpen("layout");
    open = next;
  }

  onMount(() =>
    closeOnOtherStatusPopover("layout", () => {
      if (variant === "status") open = false;
    }),
  );

  function presetIcon(presetId: string) {
    return presetId === "focus" ? Focus : PanelsTopLeft;
  }
</script>

{#if show}
  {#if showStatus}
    <button
      bind:this={triggerEl}
      type="button"
      class="workshop-status-workshop"
      class:workshop-status-workshop--open={open}
      title="Layout — {activeLabel}"
      aria-label="Layout — {activeLabel}"
      aria-haspopup="menu"
      aria-expanded={open}
      disabled={busy}
      onclick={toggleMenu}
    >
      <PanelsTopLeft size={13} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
      <span class="truncate">{activeLabel}</span>
    </button>
  {:else if showRail}
    <button
      bind:this={triggerEl}
      type="button"
      class="workshop-rail-btn workshop-rail-btn-tier-utility workshop-rail-dock-btn relative {open
        ? 'workshop-rail-workshop-btn-open'
        : ''} {activePreset?.id === 'focus' ? 'workshop-rail-btn-active-quiet' : ''}"
      title="Layout — {activeLabel}"
      aria-label="Layout — {activeLabel}"
      aria-haspopup="menu"
      aria-expanded={open}
      disabled={busy}
      onclick={() => (open = !open)}
    >
      <span class="workshop-rail-btn-icon" aria-hidden="true">
        <PanelsTopLeft size={16} strokeWidth={1.5} />
      </span>
      {#if expanded}
        <span class="workshop-rail-btn-label">Layout</span>
      {/if}
    </button>
  {:else if showSettings}
    <div class="env-preset-picker">
      <button
        type="button"
        class="env-preset-picker-trigger"
        class:env-preset-picker-trigger-open={open}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="Layout preset — {activeLabel}"
        disabled={busy}
        onclick={() => (open = !open)}
      >
        <PanelsTopLeft size={14} strokeWidth={1.75} class="shrink-0 opacity-80" aria-hidden="true" />
        <span class="env-preset-picker-label">{activeLabel}</span>
        <ChevronDown size={14} strokeWidth={2} class="env-preset-picker-chevron" aria-hidden="true" />
      </button>

      {#if open}
        <div
          class="env-preset-picker-backdrop"
          role="presentation"
          onclick={() => (open = false)}
        ></div>
        <div class="env-preset-picker-menu" role="menu" aria-label="Layout presets">
          {#each presets as preset (preset.id)}
            {@const isActive = preset.id === activePreset?.id}
            {@const Icon = presetIcon(preset.id)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={isActive}
              class="env-preset-picker-row"
              class:env-preset-picker-row-active={isActive}
              disabled={busy}
              onclick={() => void selectPreset(preset.id)}
            >
              <span class="env-preset-picker-row-icon" aria-hidden="true">
                <Icon size={14} strokeWidth={1.75} />
              </span>
              <span class="env-preset-picker-row-body">
                <span class="env-preset-picker-row-name">
                  {presetDisplayLabel(preset.id, preset.label)}
                </span>
                <span class="env-preset-picker-row-meta">{presetDescription(preset.id)}</span>
              </span>
              {#if isActive}
                <Check size={14} strokeWidth={2.5} class="env-preset-picker-row-check" aria-hidden="true" />
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if open && isFloatingMenu}
    <BodyPortal>
      <div
        class="mobile-sheet-backdrop workshop-rail-sheet-backdrop"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) open = false;
        }}
      >
        <div
          bind:this={menuEl}
          class="workshop-rail-sheet workshop-switcher-menu workshop-layout-menu"
          role="menu"
          aria-label="Layout"
        >
        <header class="workshop-switcher-header">
          <div class="min-w-0">
            <h2 class="workshop-switcher-title">Layout</h2>
            <p class="workshop-switcher-subtitle">
              Switch presets or edit destinations on the rail
            </p>
          </div>
        </header>

        {#if presets.length > 1}
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
        {:else}
          <div class="workshop-layout-single">
            <span class="workshop-layout-single-name">{activeLabel}</span>
            <span class="workshop-layout-single-meta">Active layout</span>
          </div>
        {/if}

        <div class="workshop-layout-appearance">
          <div class="workshop-switcher-divider" aria-hidden="true"></div>
          <button
            type="button"
            class="workshop-layout-chip"
            title="Flip light / dark"
            aria-label={settings.darkMode ? "Switch to light mode" : "Switch to dark mode"}
            onclick={flipMode}
          >
            <span class="workshop-layout-chip-icon" aria-hidden="true">
              {#if settings.darkMode}
                <Moon size={14} strokeWidth={2} />
              {:else}
                <Sun size={14} strokeWidth={2} />
              {/if}
            </span>
            <span class="workshop-layout-chip-body">
              <span class="workshop-layout-chip-label">Mode</span>
              <span class="workshop-layout-chip-value">{settings.darkMode ? "Dark" : "Light"}</span>
            </span>
          </button>

          <button
            type="button"
            class="workshop-layout-chip"
            class:workshop-layout-chip-open={themePickerOpen}
            title="Choose theme for this layout"
            aria-label="Theme — {activeThemeOption.label}"
            aria-haspopup="listbox"
            aria-expanded={themePickerOpen}
            disabled={themeBusy}
            onclick={() => (themePickerOpen = !themePickerOpen)}
          >
            <span class="workshop-layout-chip-swatches" aria-hidden="true">
              {#each activeThemeOption.swatches as swatch, index (index)}
                <span style:background-color={swatch}></span>
              {/each}
            </span>
            <span class="workshop-layout-chip-body">
              <span class="workshop-layout-chip-label">Theme</span>
              <span class="workshop-layout-chip-value">{activeThemeOption.label}</span>
            </span>
          </button>

          {#if themePickerOpen}
            <div class="workshop-layout-theme-popover" role="listbox" aria-label="Choose theme">
              {#each COLOR_THEME_OPTIONS as option (option.id)}
                {@const active = settings.colorTheme === option.id}
                <button
                  type="button"
                  role="option"
                  aria-selected={active}
                  class="workshop-layout-theme-card"
                  class:workshop-layout-theme-card-active={active}
                  disabled={themeBusy}
                  onclick={() => void pickTheme(option.id)}
                >
                  <span class="workshop-layout-theme-card-swatches" aria-hidden="true">
                    {#each option.swatches as swatch, index (index)}
                      <span style:background-color={swatch}></span>
                    {/each}
                  </span>
                  <span class="workshop-layout-theme-card-copy">
                    <span class="workshop-layout-theme-card-name">{option.label}</span>
                    <span class="workshop-layout-theme-card-meta">{option.tagline}</span>
                  </span>
                  {#if active}
                    <Check
                      size={14}
                      strokeWidth={2.5}
                      class="workshop-layout-theme-card-check"
                      aria-hidden="true"
                    />
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <div class="workshop-switcher-footer">
          <div class="workshop-switcher-divider" aria-hidden="true"></div>
          <button
            type="button"
            role="menuitem"
            class="workshop-switcher-action"
            onclick={openEditDestinations}
          >
            <span class="workshop-switcher-action-icon" aria-hidden="true">
              <Pencil size={14} strokeWidth={2} />
            </span>
            Edit destinations
          </button>
        </div>
        </div>
      </div>
    </BodyPortal>
  {/if}
{/if}

<style>
  .env-preset-picker {
    position: relative;
    display: inline-flex;
    max-width: 100%;
  }

  .env-preset-picker-trigger {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    max-width: 100%;
    min-height: 1.85rem;
    padding: 0.28rem 0.55rem 0.28rem 0.5rem;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    color: rgb(var(--color-surface-100));
    font-size: 0.75rem;
    font-weight: 550;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .env-preset-picker-trigger:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--color-surface-500) 65%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 45%, transparent);
  }

  .env-preset-picker-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .env-preset-picker-trigger-open {
    border-color: color-mix(in srgb, var(--color-primary-500) 45%, transparent);
  }

  .env-preset-picker-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  :global(.env-preset-picker-chevron) {
    flex-shrink: 0;
    color: rgb(var(--theme-text-tertiary));
    transition: transform 140ms ease;
  }

  .env-preset-picker-trigger-open :global(.env-preset-picker-chevron) {
    transform: rotate(180deg);
  }

  .env-preset-picker-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }

  .env-preset-picker-menu {
    position: absolute;
    top: calc(100% + 0.3rem);
    left: 0;
    z-index: 50;
    display: grid;
    min-width: min(16rem, 100vw - 2rem);
    max-width: 18rem;
    padding: 0.25rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    background: color-mix(in srgb, rgb(var(--color-surface-900)) 96%, transparent);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.35);
  }

  .env-preset-picker-row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    padding: 0.4rem 0.45rem;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .env-preset-picker-row:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
  }

  .env-preset-picker-row-active {
    background: color-mix(in srgb, var(--color-primary-600) 18%, transparent);
  }

  .env-preset-picker-row:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .env-preset-picker-row-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.4rem;
    height: 1.4rem;
    flex-shrink: 0;
    border-radius: 0.3rem;
    color: rgb(var(--theme-text-secondary));
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
  }

  .env-preset-picker-row-body {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.05rem;
  }

  .env-preset-picker-row-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .env-preset-picker-row-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-quiet));
  }

  :global(.env-preset-picker-row-check) {
    flex-shrink: 0;
    color: rgb(var(--theme-link));
  }

  .workshop-layout-single {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.35rem 0.85rem 0.55rem;
  }

  .workshop-layout-single-name {
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .workshop-layout-single-meta {
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-quiet));
  }

  .workshop-layout-appearance {
    position: relative;
    display: grid;
    gap: 0.2rem;
    flex-shrink: 0;
    padding: 0 0.5rem 0.35rem;
  }

  .workshop-layout-chip {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
    min-height: 2.25rem;
    padding: 0.35rem 0.5rem;
    border: 0;
    border-radius: 0.5rem;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .workshop-layout-chip:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .workshop-layout-chip:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .workshop-layout-chip-open {
    background: color-mix(in srgb, var(--color-primary-600) 14%, transparent);
  }

  .workshop-layout-chip-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    border-radius: 0.4rem;
    color: rgb(var(--theme-text-secondary));
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
  }

  .workshop-layout-chip-swatches {
    display: flex;
    width: 1.75rem;
    height: 1.75rem;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 0.4rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 40%, transparent);
  }

  .workshop-layout-chip-swatches span {
    display: block;
    height: 100%;
    flex: 1;
  }

  .workshop-layout-chip-body {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.05rem;
  }

  .workshop-layout-chip-label {
    font-size: 0.625rem;
    font-weight: 650;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-quiet));
  }

  .workshop-layout-chip-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .workshop-layout-theme-popover {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    max-height: min(12.5rem, 36vh);
    margin: 0.1rem 0.15rem 0.25rem;
    padding: 0.25rem;
    overflow-y: auto;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 40%, transparent);
  }

  .workshop-layout-theme-card {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    padding: 0.35rem 0.4rem;
    border: 1px solid transparent;
    border-radius: 0.45rem;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .workshop-layout-theme-card:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-surface-800) 65%, transparent);
  }

  .workshop-layout-theme-card-active {
    border-color: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-primary-600) 12%, transparent);
  }

  .workshop-layout-theme-card:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .workshop-layout-theme-card-swatches {
    display: flex;
    width: 2.1rem;
    height: 1.35rem;
    flex-shrink: 0;
    overflow: hidden;
    border-radius: 0.3rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
  }

  .workshop-layout-theme-card-swatches span {
    display: block;
    height: 100%;
    flex: 1;
  }

  .workshop-layout-theme-card-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.05rem;
  }

  .workshop-layout-theme-card-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .workshop-layout-theme-card-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-quiet));
  }

  :global(.workshop-layout-theme-card-check) {
    flex-shrink: 0;
    color: rgb(var(--theme-link));
  }
</style>
