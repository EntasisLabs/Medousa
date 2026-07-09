<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";
  import { INSTALLER_TAGLINE, truncatePath } from "../copy";
  import type { BootstrapResponse, PackageSummary } from "../types";
  import ErrorBanner from "../components/ErrorBanner.svelte";
  import OptionsPanel from "../components/OptionsPanel.svelte";
  import markUrl from "../../assets/medousa-mark.png";

  interface Props {
    bootstrap: BootstrapResponse;
    packages: PackageSummary[];
    warnings: string[];
    sizeLabel: string;
    error: string | null;
    busy?: boolean;
    showOptions: boolean;
    onInstall: () => void;
    onToggleOptions: () => void;
    onTogglePackage: (id: string) => void;
    onApplyPreset: (profileId: string) => void;
    onPickLocation: () => void;
    onRetry?: () => void;
  }

  let {
    bootstrap,
    packages,
    warnings,
    sizeLabel,
    error,
    busy = false,
    showOptions,
    onInstall,
    onToggleOptions,
    onTogglePackage,
    onApplyPreset,
    onPickLocation,
    onRetry,
  }: Props = $props();
</script>

<section class="welcome">
  <div class="welcome-scroll scroll-pane" class:expanded={showOptions}>
    <div class="welcome-stage">
      <div class="hero">
        <img class="hero-mark" src={markUrl} alt="" width="56" height="56" />
        <h1 class="hero-title">Install Medousa</h1>
        <p class="hero-lead">{INSTALLER_TAGLINE}</p>
      </div>

      <div class="express-card">
        <div class="express-label">Recommended</div>
        <div class="express-title">Express installation</div>
        <p class="express-desc">Desktop app and background service — ready in minutes.</p>
        <button type="button" class="btn-install" disabled={busy} onclick={onInstall}>
          {busy ? "Starting…" : "Install"}
        </button>
        <p class="express-meta">About {sizeLabel} disk space</p>
      </div>

      <button type="button" class="customize-link" onclick={onToggleOptions}>
        <span>{showOptions ? "Hide add-ons" : "Add channels or offline AI"}</span>
        <span class="customize-chevron" class:open={showOptions}>
          <ChevronDown size={16} strokeWidth={2} />
        </span>
      </button>

      {#if showOptions}
        <OptionsPanel
          {packages}
          {warnings}
          {onTogglePackage}
          {onApplyPreset}
          {onPickLocation}
          installRoot={bootstrap.installRoot}
          dataDir={bootstrap.dataDir}
          modelCacheDir={bootstrap.modelCacheDir}
        />
      {/if}

      <p class="location-line">
        <span class="muted">Location</span>
        <span class="path" title={bootstrap.installRoot}>
          {truncatePath(bootstrap.installRoot, 34)}
        </span>
        <button type="button" class="text-link" onclick={onPickLocation}>Change</button>
      </p>

      {#if bootstrap.versionMismatch}
        <p class="banner-warn">An update is available for your installation.</p>
      {/if}

      {#if error}
        <ErrorBanner message={error} onRetry={onRetry} />
      {/if}
    </div>
  </div>
</section>

<style>
  .welcome {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .welcome-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem 1.25rem 1.25rem;
  }

  .welcome-scroll.expanded {
    align-items: flex-start;
  }

  .welcome-stage {
    width: 100%;
    max-width: 400px;
    margin: 0 auto;
  }

  .hero {
    text-align: center;
    margin-bottom: 1.5rem;
  }

  .hero-mark {
    border-radius: 14px;
    margin-bottom: 1rem;
    box-shadow: 0 8px 32px rgb(131 68 245 / 0.18);
  }

  .hero-title {
    font-size: var(--installer-title-size);
    font-weight: 600;
    margin: 0 0 0.5rem;
  }

  .hero-lead {
    color: var(--installer-muted);
    font-size: var(--installer-body-size);
    line-height: 1.55;
    margin: 0 auto;
    max-width: 34ch;
  }

  .express-card {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 1.2rem 1.25rem 1rem;
    margin-bottom: 0.75rem;
  }

  .express-label {
    font-size: 0.6875rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--installer-accent);
    margin-bottom: 0.35rem;
  }

  .express-title {
    font-size: 1.05rem;
    font-weight: 600;
    margin-bottom: 0.35rem;
  }

  .express-desc {
    color: var(--installer-muted);
    font-size: var(--installer-body-size);
    line-height: 1.5;
    margin: 0 0 1rem;
  }

  .btn-install {
    width: 100%;
    background: var(--installer-accent);
    color: white;
    border: none;
    border-radius: var(--installer-radius-control);
    padding: 0.75rem 1rem;
    font-size: 1rem;
    font-weight: 600;
    transition: background var(--installer-motion);
  }

  .btn-install:hover:not(:disabled) {
    background: var(--installer-accent-hover);
  }

  .btn-install:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .btn-install:focus-visible {
    outline: 2px solid white;
    outline-offset: 2px;
  }

  .express-meta {
    text-align: center;
    margin: 0.65rem 0 0;
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .customize-link {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    width: 100%;
    background: none;
    border: none;
    color: var(--installer-accent);
    font-size: var(--installer-body-size);
    padding: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .customize-link:hover {
    color: var(--installer-accent-hover);
  }

  .customize-chevron {
    display: inline-flex;
    transition: transform var(--installer-motion);
  }

  .customize-chevron.open {
    transform: rotate(180deg);
  }

  .location-line {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-wrap: wrap;
    gap: 0.35rem;
    font-size: var(--installer-caption-size);
    color: var(--installer-text-secondary);
    margin: 0.75rem 0 0;
    text-align: center;
  }

  .muted {
    color: var(--installer-muted);
  }

  .path {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .text-link {
    background: none;
    border: none;
    color: var(--installer-accent);
    padding: 0;
    font-size: inherit;
  }

  .banner-warn {
    margin-top: 1rem;
    text-align: center;
    color: var(--installer-warning);
    font-size: var(--installer-caption-size);
  }
</style>
