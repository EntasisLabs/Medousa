<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import ComponentRow from "./ComponentRow.svelte";
  import { humanizeWarning, packageBlurb, PRESET_CHIPS } from "../copy";
  import type { PackageSummary } from "../types";

  interface Props {
    packages: PackageSummary[];
    warnings: string[];
    onTogglePackage: (id: string) => void;
    onApplyPreset: (profileId: string) => void;
    onPickLocation: () => void;
    installRoot: string;
    dataDir: string;
    modelCacheDir: string;
  }

  let {
    packages,
    warnings,
    onTogglePackage,
    onApplyPreset,
    onPickLocation,
    installRoot,
    dataDir,
    modelCacheDir,
  }: Props = $props();

  const optionalPackages = $derived(packages.filter((p) => p.optional));
  const channels = $derived(
    optionalPackages.filter((p) => p.category === "adapter"),
  );
  const offline = $derived(
    optionalPackages.filter(
      (p) => p.category === "model" || p.id === "local-brain",
    ),
  );
  const advanced = $derived(
    optionalPackages.filter(
      (p) =>
        p.category === "core" &&
        p.id !== "desktop" &&
        p.id !== "engine" &&
        p.id !== "local-brain",
    ),
  );
  const expansions = $derived(optionalPackages.filter((p) => p.category === "expansion"));

  const selectedAddonCount = $derived(
    optionalPackages.filter((p) => p.selected).length,
  );

  function sectionSummary(items: PackageSummary[]): string {
    const selected = items.filter((p) => p.selected);
    if (selected.length === 0) return "None selected";
    if (selected.length === 1) return selected[0].displayName;
    return `${selected.length} selected`;
  }
</script>

<div class="options-panel">
  <div class="presets">
    <span class="presets-label">Quick add</span>
    <div class="preset-chips">
      {#each PRESET_CHIPS as chip (chip.id)}
        <button
          type="button"
          class="preset-chip"
          title={chip.description}
          onclick={() => onApplyPreset(chip.profileId)}
        >
          {chip.label}
        </button>
      {/each}
    </div>
  </div>

  <details class="section">
    <summary class="section-head">
      <span class="section-title">Messaging channels</span>
      <span class="section-meta">{sectionSummary(channels)}</span>
      <ChevronDown size={16} class="chevron" />
    </summary>
    <div class="section-body">
      <p class="section-hint">Connect Medousa to the apps you already use.</p>
      {#each channels as pkg (pkg.id)}
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
  </details>

  <details class="section">
    <summary class="section-head">
      <span class="section-title">Offline AI</span>
      <span class="section-meta">{sectionSummary(offline)}</span>
      <ChevronDown size={16} class="chevron" />
    </summary>
    <div class="section-body">
      <p class="section-hint">Run AI on your machine — no cloud required. Models are large downloads.</p>
      {#each offline as pkg (pkg.id)}
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
  </details>

  {#if advanced.length > 0 || expansions.length > 0}
    <details class="section">
      <summary class="section-head">
        <span class="section-title">Advanced</span>
        <span class="section-meta">{sectionSummary([...advanced, ...expansions])}</span>
        <ChevronDown size={16} class="chevron" />
      </summary>
      <div class="section-body">
        {#each advanced as pkg (pkg.id)}
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
        {#each expansions as pkg (pkg.id)}
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
    </details>
  {/if}

  <details class="section section-location">
    <summary class="section-head">
      <span class="section-title">Install location</span>
      <ChevronDown size={16} class="chevron" />
    </summary>
    <div class="section-body location-grid">
      <div class="location-row">
        <span class="location-label">Application</span>
        <span class="location-value" title={installRoot}>{installRoot}</span>
        <button type="button" class="link-btn" onclick={onPickLocation}>Change</button>
      </div>
      <div class="location-row">
        <span class="location-label">Your data</span>
        <span class="location-value">{dataDir}</span>
      </div>
      <div class="location-row">
        <span class="location-label">AI models</span>
        <span class="location-value">{modelCacheDir}</span>
      </div>
    </div>
  </details>

  {#if selectedAddonCount > 0}
    <p class="addon-count">{selectedAddonCount} optional add-on{selectedAddonCount === 1 ? "" : "s"} selected</p>
  {/if}

  {#each warnings as warning}
    <p class="warning">{humanizeWarning(warning)}</p>
  {/each}
</div>

<style>
  .options-panel {
    margin-top: 0;
    display: grid;
    gap: 0.45rem;
  }

  .presets {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
    margin-bottom: 0.25rem;
  }

  .presets-label {
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .preset-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .preset-chip {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    color: var(--installer-text-secondary);
    border-radius: 999px;
    padding: 0.3rem 0.75rem;
    font-size: var(--installer-caption-size);
    font-weight: 500;
    transition:
      border-color var(--installer-motion),
      background var(--installer-motion),
      color var(--installer-motion);
  }

  .preset-chip:hover {
    border-color: var(--installer-accent);
    color: var(--installer-text);
    background: rgb(131 68 245 / 0.08);
  }

  .preset-chip:focus-visible {
    outline: 2px solid var(--installer-accent);
    outline-offset: 2px;
  }

  .section {
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    background: var(--installer-surface);
    overflow: hidden;
  }

  .section-head {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 0.5rem;
    padding: 0.7rem 0.85rem;
    cursor: pointer;
    list-style: none;
    user-select: none;
  }

  .section-head::-webkit-details-marker {
    display: none;
  }

  .section-title {
    font-size: var(--installer-body-size);
    font-weight: 600;
  }

  .section-meta {
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .section :global(.chevron) {
    color: var(--installer-muted);
    transition: transform var(--installer-motion);
  }

  .section[open] :global(.chevron) {
    transform: rotate(180deg);
  }

  .section-body {
    padding: 0 0.85rem 0.75rem;
    border-top: 1px solid var(--installer-border);
  }

  .section-hint {
    margin: 0.65rem 0 0.35rem;
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
    line-height: 1.45;
  }

  .location-grid {
    display: grid;
    gap: 0.75rem;
    padding-top: 0.65rem;
  }

  .location-row {
    display: grid;
    gap: 0.15rem;
  }

  .location-label {
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .location-value {
    font-size: var(--installer-body-size);
    color: var(--installer-text-secondary);
    word-break: break-all;
  }

  .link-btn {
    background: transparent;
    border: none;
    color: var(--installer-accent);
    padding: 0;
    font-size: var(--installer-caption-size);
    justify-self: start;
    margin-top: 0.15rem;
  }

  .addon-count {
    margin: 0.25rem 0 0;
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
    text-align: center;
  }

  .warning {
    color: var(--installer-warning);
    font-size: var(--installer-caption-size);
    margin: 0;
    line-height: 1.45;
  }
</style>
