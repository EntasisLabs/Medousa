<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import {
    INSTALLER_BUTTON_LABEL,
    INSTALLER_GADGETS_HIDE,
    INSTALLER_GADGETS_SHOW,
    INSTALLER_TAGLINE,
    truncatePath,
  } from "../copy";
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
  <div class="welcome-scroll scroll-pane">
    <div class="welcome-stage">
      <header class="hero">
        <img class="hero-mark" src={markUrl} alt="" width="52" height="52" />
        <p class="hero-line">{INSTALLER_TAGLINE}</p>
      </header>

      <div class="express-card">
        <button type="button" class="btn-install" disabled={busy} onclick={onInstall}>
          {busy ? "Starting…" : INSTALLER_BUTTON_LABEL}
        </button>
        <div class="express-foot">
          <span class="express-meta">About {sizeLabel}</span>
          <span class="foot-sep" aria-hidden="true">·</span>
          <span class="path" title={bootstrap.installRoot}>
            {truncatePath(bootstrap.installRoot, 28)}
          </span>
          <button type="button" class="text-link" onclick={onPickLocation}>Change</button>
        </div>
      </div>

      <button
        type="button"
        class="gadgets-link"
        aria-expanded={showOptions}
        onclick={onToggleOptions}
      >
        <span>{showOptions ? INSTALLER_GADGETS_HIDE : INSTALLER_GADGETS_SHOW}</span>
        <span class="gadgets-chevron" class:open={showOptions}>
          <ChevronRight size={15} strokeWidth={2.25} />
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
    padding: 1.5rem 1.5rem 1.25rem;
  }

  .welcome-stage {
    width: 100%;
    max-width: 380px;
    margin: 0 auto;
  }

  .hero {
    text-align: center;
    margin-bottom: 1.5rem;
  }

  .hero-mark {
    border-radius: 12px;
    margin-bottom: 1rem;
    box-shadow: 0 6px 24px rgb(131 68 245 / 0.14);
  }

  .hero-line {
    color: var(--installer-text-secondary);
    font-size: 1.0625rem;
    line-height: 1.45;
    margin: 0 auto;
    max-width: 22ch;
    font-weight: 500;
  }

  .express-card {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 1.1rem 1.15rem 0.95rem;
    margin-bottom: 0.75rem;
  }

  .btn-install {
    width: 100%;
    background: var(--installer-accent);
    color: white;
    border: none;
    border-radius: var(--installer-radius-control);
    padding: 0.72rem 1rem;
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

  .express-foot {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.7rem;
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
    text-align: center;
  }

  .foot-sep {
    opacity: 0.5;
  }

  .path {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--installer-text-secondary);
  }

  .gadgets-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.2rem;
    width: 100%;
    background: none;
    border: none;
    color: var(--installer-accent);
    font-size: var(--installer-body-size);
    padding: 0.45rem;
    margin-bottom: 0.35rem;
  }

  .gadgets-link:hover {
    color: var(--installer-accent-hover);
  }

  .gadgets-chevron {
    display: inline-flex;
    transition: transform var(--installer-motion);
  }

  .gadgets-chevron.open {
    transform: rotate(90deg);
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
