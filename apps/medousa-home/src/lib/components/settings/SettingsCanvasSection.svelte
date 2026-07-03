<script lang="ts">
  import { onMount } from "svelte";
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";

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

  onMount(() => {
    void environment.refreshCanvasStatus();
  });
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
    </dl>
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
          <header class="flex flex-wrap items-center justify-between gap-2">
            <p class="font-medium text-surface-100">{row.label}</p>
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

  .env-status-card {
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
    padding: 0.75rem 0.875rem;
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
