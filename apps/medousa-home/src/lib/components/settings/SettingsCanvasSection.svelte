<script lang="ts">
  import { onMount } from "svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import CanvasNavDestinationsPanel from "$lib/components/settings/CanvasNavDestinationsPanel.svelte";
  import CanvasAddViewForm from "$lib/components/settings/CanvasAddViewForm.svelte";
  import CanvasAddLayoutPresetForm from "$lib/components/settings/CanvasAddLayoutPresetForm.svelte";
  import CanvasViewsPanel from "$lib/components/settings/CanvasViewsPanel.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { resolveEnvironmentTheme } from "$lib/utils/environmentTheme";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import { ChevronDown, LayoutGrid } from "@lucide/svelte";

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
  const activePreset = $derived(
    spec?.layoutPresets?.find((preset) => preset.active) ??
      spec?.layoutPresets?.find((preset) => preset.id === spec?.activePresetId) ??
      null,
  );
  const activePresetSurfaceIds = $derived(new Set(activePreset?.surfaces ?? []));
  const resolvedTheme = $derived(
    resolveEnvironmentTheme(
      spec,
      workshops.activeColorThemeId ?? settings.colorTheme,
      workshops.activeBrandColor,
      settings.darkMode,
    ),
  );

  onMount(() => {
    void environment.refreshCanvasStatus();
  });

  let confirmDeleteSurfaceId = $state<string | null>(null);
  let editingSurfaceId = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state<string | null>(null);
  let advancedOpen = $state(false);
  let mobileHomeBusy = $state(false);
  let mobileHomeError = $state<string | null>(null);

  const mobileHomeValue = $derived(environment.mobileDefaultHome);

  function navVisibleFor(surfaceId: string): boolean {
    const statusRow = canvasStatus?.customSurfaces.find((row) => row.surfaceId === surfaceId);
    if (statusRow) return statusRow.navVisible;
    return activePresetSurfaceIds.has(surfaceId);
  }

  async function deleteCustomView(surfaceId: string, label: string) {
    deleteBusy = true;
    deleteError = null;
    try {
      await environment.removeCustomSurface(surfaceId);
      confirmDeleteSurfaceId = null;
    } catch (err) {
      deleteError = err instanceof Error ? err.message : String(err);
    } finally {
      deleteBusy = false;
    }
  }

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
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Canvas</h2>
    <p class="workshop-faint mt-1 text-sm">
      Create rooms, choose a layout preset, and choose what appears in the rail.
    </p>
  </header>

  <div class="canvas-settings-block">
    <h3 class="canvas-settings-heading">Your views</h3>
    <p class="canvas-settings-lead">
      Rooms you created — open to add widgets, toggle the rail, or edit and remove.
    </p>
    <CanvasViewsPanel
      surfaces={customSurfaces}
      {navVisibleFor}
      {editingSurfaceId}
      {confirmDeleteSurfaceId}
      {deleteBusy}
      onRequestEdit={(surfaceId) => {
        confirmDeleteSurfaceId = null;
        editingSurfaceId = surfaceId;
      }}
      onCancelEdit={() => {
        editingSurfaceId = null;
      }}
      onRequestDelete={(surfaceId) => {
        editingSurfaceId = null;
        confirmDeleteSurfaceId = surfaceId;
      }}
      onConfirmDelete={(surfaceId, label) => void deleteCustomView(surfaceId, label)}
      onCancelDelete={() => {
        confirmDeleteSurfaceId = null;
      }}
    />
    <CanvasAddViewForm />
  </div>

  <div class="canvas-settings-block">
    <h3 class="canvas-settings-heading">Layout preset</h3>
    <p class="canvas-settings-lead">
      Switch between saved nav profiles, or save the current destinations as a new layout.
    </p>
    <EnvironmentPresetSwitcher />
    <CanvasAddLayoutPresetForm />
    {#if activePreset?.id === "focus"}
      <p class="canvas-settings-note">
        Focus is active. Custom views listed in the preset still show in nav.
      </p>
    {/if}
  </div>

  {#if spec}
    <div class="canvas-settings-block">
      <h3 class="canvas-settings-heading">Nav destinations</h3>
      <p class="canvas-settings-lead">
        Show or hide built-in views in the rail. Custom rooms are managed above — switch presets for
        quick profiles like Focus.
      </p>
      <CanvasNavDestinationsPanel {spec} />
    </div>

    <div class="canvas-settings-block">
      <h3 class="canvas-settings-heading">Mobile Home button</h3>
      <p class="canvas-settings-lead">
        What the phone Home tab shows. Native Home is the default work/glance screen. Opening a custom
        view from More is temporary — tap Home again to leave it.
      </p>
      <label class="canvas-mobile-home-field">
        <span class="sr-only">Mobile Home surface</span>
        <select
          class="select"
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
      {#if mobileHomeError}
        <p class="canvas-settings-note text-warning-200">{mobileHomeError}</p>
      {/if}
    </div>
  {/if}

  <div class="canvas-pin-callout">
    <span class="canvas-pin-callout-icon" aria-hidden="true">
      <LayoutGrid size={18} strokeWidth={1.75} />
    </span>
    <div class="canvas-pin-callout-copy">
      <p class="canvas-pin-callout-title">Pin widgets on the canvas</p>
      <p class="canvas-pin-callout-body">
        Open a view, tap <strong>Edit layout</strong>, then <strong>Add widget</strong> — presentations,
        vault notes, or Spotify / Apple embeds.
      </p>
      <button
        type="button"
        class="canvas-pin-callout-link"
        onclick={() => void openUrlInDefaultBrowser(LAYOUT_EDIT_DOC)}
      >
        Layout edit guide
      </button>
    </div>
  </div>

  {#if deleteError}
    <p class="canvas-settings-error">{deleteError}</p>
  {/if}

  {#if pending}
    <div class="env-pending-card mt-4">
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

  <details class="canvas-advanced" bind:open={advancedOpen}>
    <summary class="canvas-advanced-summary">
      <span>Advanced</span>
      <ChevronDown size={16} strokeWidth={2} class="canvas-advanced-chevron" aria-hidden="true" />
    </summary>

    <div class="canvas-advanced-body">
      {#if spec}
        <dl class="settings-kv">
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
            <dd class="env-theme-row">
              <span>{resolvedTheme.paletteLabel}</span>
              {#if resolvedTheme.brandColor}
                <span
                  class="env-theme-swatch"
                  style:background={resolvedTheme.brandColor}
                  title={resolvedTheme.brandColor}
                ></span>
              {/if}
              {#if resolvedTheme.tagline}
                <span class="workshop-faint"> — {resolvedTheme.tagline}</span>
              {/if}
            </dd>
          </div>
        </dl>
      {/if}

      {#if environment.canvasStatusLoading}
        <p class="workshop-faint mt-3 text-xs">Loading live canvas status…</p>
      {:else if environment.canvasStatusError}
        <p class="canvas-settings-error">{environment.canvasStatusError}</p>
      {:else if canvasStatus && canvasStatus.customSurfaces.length > 0}
        <div class="mt-4 space-y-3">
          <h4 class="canvas-advanced-subheading">Live surface status</h4>
          {#each canvasStatus.customSurfaces as row (row.surfaceId)}
            <article class="env-status-card">
              <header class="env-status-card-header">
                <div class="min-w-0">
                  <p class="font-medium text-surface-100">{row.label}</p>
                  <p class="workshop-faint text-xs">{row.surfaceId}</p>
                </div>
                <span class="env-status-pill" class:env-status-pill-on={row.navVisible}>
                  {row.navVisible ? "In nav" : "Hidden from nav"}
                </span>
              </header>
              <dl class="env-status-kv mt-2">
                <div>
                  <dt>Components</dt>
                  <dd>
                    {row.components.length
                      ? row.components.map((c) => c.componentId).join(", ")
                      : "none"}
                  </dd>
                </div>
                {#each row.components as comp (comp.componentId)}
                  {#if comp.runtime?.lastError || (comp.runtime?.storeKeyCount ?? 0) > 0}
                    <div class="env-status-runtime">
                      <dt>{comp.componentId}</dt>
                      <dd>
                        {#if comp.runtime?.lastError}
                          <span class="env-status-error" title={comp.runtime.lastError}>
                            Last error: {comp.runtime.lastError}
                          </span>
                        {/if}
                        {#if (comp.runtime?.storeKeyCount ?? 0) > 0}
                          <span class="env-status-muted">
                            Store keys: {comp.runtime?.storeKeyCount}
                          </span>
                        {/if}
                      </dd>
                    </div>
                  {/if}
                {/each}
                <div>
                  <dt>Feeds</dt>
                  <dd>{row.subscribedFeedIds.join(", ") || "none"}</dd>
                </div>
                {#if row.feedStatus.length > 0}
                  <div>
                    <dt>Last tick</dt>
                    <dd>
                      {row.feedStatus
                        .map((feed) => `${feed.feedId}: ${feed.lastEmittedAtUtc ?? "never"}`)
                        .join(" · ")}
                    </dd>
                  </div>
                {/if}
                {#if row.recurringBindings.length > 0}
                  <div>
                    <dt>Recurring</dt>
                    <dd>
                      {row.recurringBindings
                        .map((binding) => binding.recurringId)
                        .join(", ")}
                    </dd>
                  </div>
                {/if}
              </dl>
              {#if row.feedMismatches.length > 0}
                <ul class="env-status-warnings mt-2 text-xs text-warning-300">
                  {#each row.feedMismatches as warning (warning)}
                    <li>{warning}</li>
                  {/each}
                </ul>
              {/if}
            </article>
          {/each}
          {#if canvasStatus.hints.length > 0}
            <ul class="workshop-faint text-xs">
              {#each canvasStatus.hints as hint (hint)}
                <li>{hint}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
  </details>

  <footer class="mt-6">
    <button
      type="button"
      class="btn btn-sm btn-ghost workshop-faint"
      onclick={() => void openUrlInDefaultBrowser(CUSTOM_VIEWS_DOC)}
    >
      Learn about custom views
    </button>
  </footer>
</section>

<style>
  .canvas-settings-block {
    margin-top: 1.25rem;
  }

  .canvas-settings-heading {
    margin: 0 0 0.55rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .canvas-settings-lead,
  .canvas-settings-note {
    margin: 0 0 0.65rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .canvas-settings-note {
    margin-top: 0.55rem;
    margin-bottom: 0;
  }

  .canvas-pin-callout {
    display: flex;
    gap: 0.75rem;
    margin-top: 1.25rem;
    padding: 0.85rem 0.9rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
  }

  .canvas-pin-callout-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    border-radius: 0.5rem;
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .canvas-pin-callout-copy {
    min-width: 0;
  }

  .canvas-pin-callout-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .canvas-pin-callout-body {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .canvas-pin-callout-body strong {
    font-weight: 600;
    color: rgb(var(--color-surface-300));
  }

  .canvas-pin-callout-link {
    margin-top: 0.45rem;
    border: 0;
    padding: 0;
    font: inherit;
    font-size: 0.75rem;
    color: rgb(var(--color-primary-300));
    background: transparent;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .canvas-settings-error {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .canvas-advanced {
    margin-top: 1.25rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 25%, transparent);
  }

  .canvas-advanced-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.65rem 0.85rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    list-style: none;
  }

  .canvas-advanced-summary::-webkit-details-marker {
    display: none;
  }

  :global(.canvas-advanced-chevron) {
    transition: transform 160ms ease;
  }

  .canvas-advanced[open] :global(.canvas-advanced-chevron) {
    transform: rotate(180deg);
  }

  .canvas-advanced-body {
    padding: 0 0.85rem 0.85rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 40%, transparent);
  }

  .canvas-advanced-subheading {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-200));
  }

  .settings-kv {
    display: grid;
    gap: 0.5rem;
    padding-top: 0.75rem;
    font-size: 0.8125rem;
  }

  .settings-kv dt {
    color: rgb(var(--color-surface-500));
  }

  .settings-kv dd {
    margin: 0;
    color: rgb(var(--color-surface-100));
  }

  .env-theme-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }

  .env-theme-swatch {
    display: inline-block;
    width: 0.875rem;
    height: 0.875rem;
    border-radius: 9999px;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 50%, transparent);
  }

  .env-status-card {
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
    padding: 0.75rem 0.875rem;
  }

  .env-status-card-header {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .env-status-kv {
    display: grid;
    gap: 0.35rem;
    font-size: 0.75rem;
  }

  .env-status-kv dt {
    color: rgb(var(--color-surface-500));
  }

  .env-status-kv dd {
    margin: 0;
    color: rgb(var(--color-surface-200));
    overflow-wrap: anywhere;
  }

  .env-status-pill {
    border-radius: 999px;
    padding: 0.125rem 0.5rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
    background: color-mix(in srgb, var(--color-surface-700) 50%, transparent);
  }

  .env-status-pill-on {
    color: rgb(var(--color-success-300));
    background: color-mix(in srgb, var(--color-success-500) 12%, transparent);
  }

  .env-status-warnings {
    margin: 0;
    padding-left: 1rem;
  }

  .env-status-runtime {
    grid-column: 1 / -1;
  }

  .env-status-error {
    display: block;
    color: rgb(var(--color-error-300));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .env-status-muted {
    display: block;
    color: rgb(var(--color-surface-500));
    font-size: 0.6875rem;
  }

  .env-pending-card {
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
    padding: 0.875rem 1rem;
  }

  .env-pending-errors {
    margin: 0;
    padding-left: 1rem;
  }
</style>
