<script lang="ts">
  import { onMount } from "svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import CanvasAddLayoutPresetForm from "$lib/components/settings/CanvasAddLayoutPresetForm.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { resolveEnvironmentTheme } from "$lib/utils/environmentTheme";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    /** Denser tile layout used inside Preferences. */
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const CUSTOM_VIEWS_DOC =
    "https://github.com/EntasisLabs/Medousa/blob/main/docs/cookbook/custom-views-and-canvas.md";
  const LAYOUT_EDIT_DOC =
    "https://github.com/EntasisLabs/Medousa/blob/main/docs/cookbook/canvas-layout-edit.md";

  const spec = $derived(environment.spec);
  const pending = $derived(environment.pendingProposal);
  const customSurfaces = $derived(
    (spec?.surfaces ?? []).filter((surface) => surface.kind === "custom"),
  );
  const canvasStatus = $derived(environment.canvasStatus);
  const resolvedTheme = $derived(
    resolveEnvironmentTheme(
      spec,
      workshops.activeColorThemeId ?? settings.colorTheme,
      workshops.activeBrandColor,
      settings.darkMode,
    ),
  );

  let advancedOpen = $state(false);
  let chromeOpen = $state(false);
  let mobileHomeBusy = $state(false);
  let mobileHomeError = $state<string | null>(null);
  let chromeBusy = $state(false);
  let chromeError = $state<string | null>(null);

  const mobileHomeValue = $derived(environment.mobileDefaultHome);
  const desktopChrome = $derived(environment.desktopShellChrome);

  onMount(() => {
    void environment.refreshCanvasStatus();
  });

  async function onMobileHomeChange(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    mobileHomeBusy = true;
    mobileHomeError = null;
    try {
      await environment.setMobileDefaultHome(value);
      layout.clearMobileSurfaceOverride();
    } catch (err) {
      mobileHomeError = err instanceof Error ? err.message : String(err);
    } finally {
      mobileHomeBusy = false;
    }
  }

  async function patchChrome(
    patch: Parameters<typeof environment.patchShellChromeDesktop>[0],
  ) {
    chromeBusy = true;
    chromeError = null;
    try {
      await environment.patchShellChromeDesktop(patch);
    } catch (err) {
      chromeError = err instanceof Error ? err.message : String(err);
    } finally {
      chromeBusy = false;
    }
  }
</script>

<div class="room-shell" class:room-shell-compact={compact}>
  <div class="room-shell-grid">
    <label class="room-shell-tile">
      <span class="room-shell-copy">
        <span class="room-shell-title">Left rail</span>
        <span class="workshop-faint room-shell-meta">Destinations list</span>
      </span>
      <input
        type="checkbox"
        class="room-shell-switch"
        checked={desktopChrome.navStyle === "rail"}
        disabled={chromeBusy}
        onchange={(event) =>
          void patchChrome({
            navStyle: (event.currentTarget as HTMLInputElement).checked ? "rail" : "compact",
          })}
      />
    </label>

    <label class="room-shell-tile">
      <span class="room-shell-copy">
        <span class="room-shell-title">Vault chat</span>
        <span class="workshop-faint room-shell-meta">Floating ask on notes</span>
      </span>
      <input
        type="checkbox"
        class="room-shell-switch"
        checked={desktopChrome.vaultChatFab}
        disabled={chromeBusy}
        onchange={(event) =>
          void patchChrome({
            vaultChatFab: (event.currentTarget as HTMLInputElement).checked,
          })}
      />
    </label>

    <label class="room-shell-tile">
      <span class="room-shell-copy">
        <span class="room-shell-title">Vault sidebar</span>
        <span class="workshop-faint room-shell-meta">Notes & browse</span>
      </span>
      <input
        type="checkbox"
        class="room-shell-switch"
        checked={desktopChrome.vaultSidebar === "visible"}
        disabled={chromeBusy}
        onchange={(event) =>
          void patchChrome({
            vaultSidebar: (event.currentTarget as HTMLInputElement).checked
              ? "visible"
              : "hidden",
          })}
      />
    </label>

    <label class="room-shell-tile room-shell-tile-select">
      <span class="room-shell-copy">
        <span class="room-shell-title">Mobile Home</span>
        <span class="workshop-faint room-shell-meta">Phone Home tab</span>
      </span>
      <select
        class="select room-shell-select"
        value={mobileHomeValue}
        disabled={mobileHomeBusy}
        onchange={(event) => void onMobileHomeChange(event)}
      >
        <option value="home">Native Home</option>
        {#each customSurfaces as surface (surface.id)}
          <option value={surface.id}>{surface.label}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if chromeError}
    <p class="room-shell-note text-warning-200">{chromeError}</p>
  {/if}
  {#if mobileHomeError}
    <p class="room-shell-note text-warning-200">{mobileHomeError}</p>
  {/if}

  {#if pending}
    <div class="env-pending-card room-shell-pending">
      <p class="text-sm font-medium text-surface-100">Pending workshop layout</p>
      <p class="workshop-faint mt-1 text-xs">{pending.diffSummary}</p>
      <p class="workshop-faint mt-1 text-xs">Proposed by {pending.proposedBy}</p>
      {#if pending.errors.length > 0}
        <ul class="env-pending-errors mt-2 text-xs text-error-300">
          {#each pending.errors as error (error)}
            <li>{error}</li>
          {/each}
        </ul>
      {/if}
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={environment.pendingBusy || pending.errors.length > 0}
          onclick={() => void environment.applyPendingProposal()}
        >
          Apply layout
        </button>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          disabled={environment.pendingBusy}
          onclick={() => void environment.dismissPendingProposal()}
        >
          Dismiss
        </button>
      </div>
    </div>
  {/if}

  <details class="room-shell-chrome-more" bind:open={chromeOpen}>
    <summary class="room-shell-chrome-summary">
      <span>Layouts & advanced</span>
      <ChevronDown size={14} strokeWidth={2} class="room-shell-chevron" aria-hidden="true" />
    </summary>
    <div class="room-shell-chrome-body">
      <p class="workshop-faint room-shell-hint">
        Rail destinations: status bar layout menu → Edit destinations.
      </p>

      {#if (spec?.layoutPresets?.length ?? 0) > 0}
        <div class="room-shell-layouts">
          <div class="room-shell-layouts-row">
            <EnvironmentPresetSwitcher variant="settings" />
            <CanvasAddLayoutPresetForm />
          </div>
        </div>
      {/if}

      <details class="room-shell-advanced" bind:open={advancedOpen}>
        <summary class="room-shell-advanced-summary">
          <span>Diagnostics</span>
          <ChevronDown size={14} strokeWidth={2} class="room-shell-chevron" aria-hidden="true" />
        </summary>
        <div class="room-shell-advanced-body">
          {#if spec}
            <dl class="room-shell-kv">
              <div>
                <dt>Surfaces</dt>
                <dd>{spec.surfaces.length}</dd>
              </div>
              <div>
                <dt>Components</dt>
                <dd>{spec.components.length}</dd>
              </div>
              <div>
                <dt>Environment theme</dt>
                <dd class="room-shell-theme-row">
                  <span>{resolvedTheme.paletteLabel}</span>
                  {#if resolvedTheme.brandColor}
                    <span
                      class="room-shell-swatch"
                      style:background={resolvedTheme.brandColor}
                      title={resolvedTheme.brandColor}
                    ></span>
                  {/if}
                </dd>
              </div>
            </dl>
          {/if}

          {#if environment.canvasStatusLoading}
            <p class="workshop-faint mt-3 text-xs">Loading live canvas status…</p>
          {:else if environment.canvasStatusError}
            <p class="room-shell-error">{environment.canvasStatusError}</p>
          {:else if canvasStatus && canvasStatus.customSurfaces.length > 0}
            <div class="mt-3 space-y-2">
              <p class="text-xs font-semibold text-surface-300">Live surface status</p>
              {#each canvasStatus.customSurfaces as row (row.surfaceId)}
                <div class="room-shell-status">
                  <div class="flex items-center justify-between gap-2">
                    <span class="text-sm text-surface-100">{row.label}</span>
                    <span class="workshop-faint text-xs">
                      {row.navVisible ? "In nav" : "Hidden"}
                    </span>
                  </div>
                  <p class="workshop-faint mt-0.5 text-xs">{row.surfaceId}</p>
                </div>
              {/each}
            </div>
          {/if}

          <div class="room-shell-docs">
            <button
              type="button"
              class="btn btn-sm btn-ghost workshop-faint"
              onclick={() => void openUrlInDefaultBrowser(LAYOUT_EDIT_DOC)}
            >
              Layout edit guide
            </button>
            <button
              type="button"
              class="btn btn-sm btn-ghost workshop-faint"
              onclick={() => void openUrlInDefaultBrowser(CUSTOM_VIEWS_DOC)}
            >
              Custom views
            </button>
          </div>
        </div>
      </details>
    </div>
  </details>
</div>

<style>
  .room-shell {
    --room-gap: var(--prefs-gap, 0.5rem);
    --room-tile-radius: var(--prefs-tile-radius, 0.65rem);
    --room-tile-pad: var(--prefs-tile-pad, 0.55rem 0.75rem);
    --room-tile-min-h: var(--prefs-tile-min-h, 3.25rem);
    --room-tile-border: var(--prefs-tile-border, rgb(var(--color-surface-500) / 0.32));
    --room-tile-bg: var(--prefs-tile-bg, rgb(var(--color-surface-900) / 0.28));
  }

  .room-shell-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--room-gap);
  }

  @media (min-width: 720px) {
    .room-shell-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .room-shell-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--room-tile-min-h);
    padding: var(--room-tile-pad);
    border-radius: var(--room-tile-radius);
    border: 1px solid var(--room-tile-border);
    background: var(--room-tile-bg);
    cursor: pointer;
  }

  .room-shell-tile:hover {
    border-color: rgb(var(--shell-border, var(--color-surface-500)) / 0.48);
  }

  .room-shell-tile-select {
    cursor: default;
  }

  .room-shell-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.05rem;
  }

  .room-shell-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .room-shell-meta {
    font-size: 0.68rem;
  }

  .room-shell-select {
    max-width: 8.5rem;
    min-width: 0;
    flex: 0 1 auto;
    padding-block: 0.2rem;
    font-size: 0.72rem;
  }

  .room-shell-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .room-shell-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .room-shell-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .room-shell-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .room-shell-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .room-shell-switch:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.7);
    outline-offset: 2px;
  }

  .room-shell-note {
    margin: 0.45rem 0 0;
    font-size: 0.75rem;
  }

  .room-shell-hint {
    margin: 0 0 0.45rem;
    font-size: 0.6875rem;
  }

  .room-shell-chrome-more {
    margin-top: 0;
    border-radius: var(--room-tile-radius);
    border: 1px solid var(--room-tile-border);
    background: var(--room-tile-bg);
  }

  .room-shell-chrome-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--room-tile-min-h);
    padding: var(--room-tile-pad);
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-400)));
    cursor: pointer;
    list-style: none;
  }

  .room-shell-chrome-summary::-webkit-details-marker {
    display: none;
  }

  .room-shell-chrome-body {
    padding: 0 0.65rem 0.65rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.24);
  }

  .room-shell-layouts {
    margin-top: 0.35rem;
  }

  .room-shell-layouts-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 0.5rem 0.65rem;
  }

  .room-shell-pending {
    margin-top: 0.55rem;
    padding: 0.7rem 0.8rem;
  }

  .room-shell-advanced {
    margin-top: 0.55rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.25);
  }

  .room-shell-advanced-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    font-size: 0.7rem;
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-400)));
    cursor: pointer;
    list-style: none;
  }

  .room-shell-advanced-summary::-webkit-details-marker {
    display: none;
  }

  :global(.room-shell-chevron) {
    transition: transform 160ms ease;
  }

  .room-shell-chrome-more[open] :global(.room-shell-chevron),
  .room-shell-advanced[open] :global(.room-shell-chevron) {
    transform: rotate(180deg);
  }

  .room-shell-advanced-body {
    padding: 0 0.6rem 0.6rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.24);
  }

  .room-shell-kv {
    display: grid;
    gap: 0.35rem;
    padding-top: 0.5rem;
    font-size: 0.75rem;
  }

  .room-shell-kv dt {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .room-shell-kv dd {
    margin: 0;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .room-shell-theme-row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .room-shell-swatch {
    width: 0.875rem;
    height: 0.875rem;
    border-radius: 999px;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
  }

  .room-shell-status {
    padding: 0.45rem 0.55rem;
    border-radius: 0.45rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-900)) / 0.35);
  }

  .room-shell-docs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.5rem;
    margin-top: 0.65rem;
    padding-top: 0.55rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
  }

  .room-shell-error {
    margin: 0.55rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
