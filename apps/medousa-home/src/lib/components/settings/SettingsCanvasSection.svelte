<script lang="ts">
  import { onMount } from "svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { resolveEnvironmentTheme } from "$lib/utils/environmentTheme";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";

  const CUSTOM_VIEWS_DOC =
    "https://github.com/EntasisLabs/Medousa/blob/main/docs/cookbook/custom-views-and-canvas.md";

  const spec = $derived(environment.spec);
  const pending = $derived(environment.pendingProposal);
  const customSurfaces = $derived(
    (spec?.surfaces ?? []).filter((surface) => surface.kind === "custom"),
  );
  const canvasStatus = $derived(environment.canvasStatus);
  const activePresetId = $derived(spec?.activePresetId ?? "default");
  const activePreset = $derived(
    spec?.layoutPresets?.find((preset) => preset.active) ??
      spec?.layoutPresets?.find((preset) => preset.id === activePresetId) ??
      null,
  );
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
  let deleteBusy = $state(false);
  let deleteError = $state<string | null>(null);

  async function deleteCustomView(surfaceId: string, label: string) {
    const confirmed = window.confirm(
      `Remove “${label}” from your canvas? Widget HTML files stay in your library.`,
    );
    if (!confirmed) return;
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
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Canvas</h2>
    <p class="workshop-faint mt-1 text-sm">
      Layout presets, custom surfaces, and agent-proposed environment changes.
    </p>
  </header>

  <div class="mt-4">
    <EnvironmentPresetSwitcher />
  </div>

  {#if activePreset?.id === "focus"}
    <p class="workshop-faint mt-3 text-xs">
      Focus hides web and workshop noise — custom views in the active preset still appear in nav.
    </p>
  {/if}

  {#if spec}
    <dl class="settings-kv mt-6">
      <div>
        <dt>Active preset</dt>
        <dd>{presetDisplayLabel(activePresetId, activePreset?.label)}</dd>
      </div>
      <div>
        <dt>Surfaces</dt>
        <dd>{spec.surfaces.length}</dd>
      </div>
      <div>
        <dt>Components</dt>
        <dd>{spec.components.length}</dd>
      </div>
      <div>
        <dt>Custom surfaces</dt>
        <dd>{customSurfaces.map((s) => s.id).join(", ") || "none"}</dd>
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
    {#if customSurfaces.length > 0}
      <ul class="env-surface-meta mt-3 text-xs">
        {#each customSurfaces as surface (surface.id)}
          <li>
            <span class="text-surface-200">{surface.label}</span>
            <span class="workshop-faint"> · icon: {surface.icon}</span>
          </li>
        {/each}
      </ul>
      <p class="workshop-faint mt-2 text-xs">
        Ask Medousa to change a nav icon (e.g. “set braindump icon to pen-line”) or retheme your
        views.
      </p>
    {/if}
  {/if}

  {#if deleteError}
    <p class="mt-3 text-xs text-error-300">{deleteError}</p>
  {/if}

  {#if environment.canvasStatusLoading}
    <p class="workshop-faint mt-4 text-xs">Loading live canvas status…</p>
  {:else if environment.canvasStatusError}
    <p class="mt-4 text-xs text-error-300">{environment.canvasStatusError}</p>
  {:else if canvasStatus && canvasStatus.customSurfaces.length > 0}
    <div class="mt-6 space-y-3">
      <h3 class="text-sm font-medium text-surface-100">Custom surface status</h3>
      {#each canvasStatus.customSurfaces as row (row.surfaceId)}
        <article class="env-status-card">
          <header class="env-status-card-header">
            <div class="min-w-0">
              <p class="font-medium text-surface-100">{row.label}</p>
              <p class="workshop-faint text-xs">{row.surfaceId}</p>
            </div>
            <div class="env-status-card-actions">
              <span class="env-status-pill" class:env-status-pill-on={row.navVisible}>
                {row.navVisible ? "In nav" : "Hidden from nav"}
              </span>
              {#if confirmDeleteSurfaceId === row.surfaceId}
                <button
                  type="button"
                  class="btn btn-xs btn-ghost text-error-300"
                  disabled={deleteBusy}
                  onclick={() => void deleteCustomView(row.surfaceId, row.label)}
                >
                  {deleteBusy ? "Removing…" : "Confirm remove"}
                </button>
                <button
                  type="button"
                  class="btn btn-xs btn-ghost"
                  disabled={deleteBusy}
                  onclick={() => {
                    confirmDeleteSurfaceId = null;
                  }}
                >
                  Cancel
                </button>
              {:else}
                <button
                  type="button"
                  class="btn btn-xs btn-ghost text-error-300"
                  disabled={deleteBusy}
                  onclick={() => {
                    confirmDeleteSurfaceId = row.surfaceId;
                  }}
                >
                  Remove view
                </button>
              {/if}
            </div>
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

  {#if pending}
    <div class="env-pending-card mt-6">
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
  .settings-kv {
    display: grid;
    gap: 0.5rem;
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

  .env-surface-meta {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
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

  .env-status-card-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
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
