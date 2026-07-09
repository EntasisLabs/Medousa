<script lang="ts">
  import ComponentRow from "../components/ComponentRow.svelte";
  import ErrorBanner from "../components/ErrorBanner.svelte";
  import { categoryLabel, humanizeWarning, packageBlurb, truncatePath } from "../copy";
  import type { BootstrapResponse, PackageSummary } from "../types";

  interface Props {
    bootstrap: BootstrapResponse;
    packages: PackageSummary[];
    warnings: string[];
    error: string | null;
    sizeLabel: string;
    hasChanges: boolean;
    onTogglePackage: (id: string) => void;
    onPickLocation: () => void;
    onApply: () => void;
    onRetry?: () => void;
  }

  let {
    bootstrap,
    packages,
    warnings,
    error,
    sizeLabel,
    hasChanges,
    onTogglePackage,
    onPickLocation,
    onApply,
    onRetry,
  }: Props = $props();

  const grouped = $derived.by(() => {
    const groups = new Map<string, PackageSummary[]>();
    for (const pkg of packages) {
      const key = pkg.categoryLabel || categoryLabel(pkg.category);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(pkg);
    }
    return [...groups.entries()];
  });
</script>

<section class="manage">
  <header class="manage-header">
    <h1>Manage installation</h1>
    <p class="lead">Add or remove components. Core app and service always stay installed.</p>
  </header>

  <div class="manage-scroll scroll-pane">
    {#each grouped as [label, items] (label)}
      <div class="group-label">{label}</div>
      <div class="group-card">
        {#each items as pkg (pkg.id)}
          <ComponentRow
            name={pkg.displayName}
            description={packageBlurb(pkg.id, pkg.displayName)}
            sizeLabel={pkg.sizeLabel}
            selected={pkg.selected}
            optional={pkg.optional}
            installed={pkg.installed}
            updateAvailable={pkg.updateAvailable}
            ontoggle={() => onTogglePackage(pkg.id)}
          />
        {/each}
      </div>
    {/each}

    <div class="location-block">
      <div class="group-label">Location</div>
      <div class="group-card location-card">
        <p class="location-path" title={bootstrap.installRoot}>
          {truncatePath(bootstrap.installRoot, 52)}
        </p>
        <button type="button" class="link-btn" onclick={onPickLocation}>Change install folder</button>
      </div>
    </div>

    {#each warnings as warning}
      <p class="warning">{humanizeWarning(warning)}</p>
    {/each}

    {#if error}
      <ErrorBanner message={error} onRetry={onRetry} />
    {/if}
  </div>

  <footer class="manage-footer">
    <span class="size-label">Total: {sizeLabel}</span>
    <button
      type="button"
      class="apply-btn"
      disabled={!hasChanges}
      onclick={onApply}
    >
      Apply changes
    </button>
  </footer>
</section>

<style>
  .manage {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .manage-header {
    flex-shrink: 0;
    margin-bottom: 0.75rem;
  }

  .manage-header h1 {
    font-size: var(--installer-title-size);
    font-weight: 600;
    margin: 0 0 0.35rem;
  }

  .lead {
    color: var(--installer-muted);
    font-size: var(--installer-body-size);
    margin: 0;
    line-height: 1.5;
  }

  .manage-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 0.15rem;
  }

  .group-label {
    font-size: var(--installer-caption-size);
    font-weight: 600;
    color: var(--installer-muted);
    margin: 0.75rem 0 0.4rem;
  }

  .group-label:first-child {
    margin-top: 0;
  }

  .group-card {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 0.25rem 0.85rem;
  }

  .location-block {
    margin-top: 0.5rem;
  }

  .location-card {
    padding: 0.85rem;
    display: grid;
    gap: 0.35rem;
  }

  .location-path {
    margin: 0;
    font-size: var(--installer-body-size);
    color: var(--installer-text-secondary);
    word-break: break-all;
  }

  .link-btn {
    background: transparent;
    border: none;
    color: var(--installer-accent);
    padding: 0;
    font-size: var(--installer-body-size);
    justify-self: start;
  }

  .warning {
    color: var(--installer-warning);
    font-size: var(--installer-caption-size);
    margin: 0.75rem 0 0;
  }

  .manage-footer {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding-top: 0.85rem;
    margin-top: 0.5rem;
    border-top: 1px solid var(--installer-border);
  }

  .size-label {
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .apply-btn {
    background: var(--installer-accent);
    color: white;
    border: none;
    border-radius: var(--installer-radius-control);
    padding: 0.6rem 1.25rem;
    font-weight: 600;
    font-size: var(--installer-body-size);
  }

  .apply-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .apply-btn:focus-visible {
    outline: 2px solid white;
    outline-offset: 2px;
  }
</style>
