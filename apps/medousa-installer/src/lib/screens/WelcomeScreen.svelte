<script lang="ts">
  import { INSTALLER_TAGLINE, truncatePath } from "../copy";
  import type { BootstrapResponse } from "../types";
  import markUrl from "../../assets/medousa-mark.png";

  interface Props {
    bootstrap: BootstrapResponse;
    onExpressInstall: () => void;
    onCustomize: () => void;
    onPickLocation: () => void;
    busy?: boolean;
  }

  let { bootstrap, onExpressInstall, onCustomize, onPickLocation, busy = false }: Props =
    $props();
</script>

<section class="welcome">
  <div class="hero">
    <img class="hero-mark" src={markUrl} alt="" width="56" height="56" />
    <h1 class="hero-title">Install Medousa</h1>
    <p class="hero-lead">{INSTALLER_TAGLINE}</p>
  </div>

  <div class="express-card">
    <div class="express-label">Recommended</div>
    <div class="express-title">Express installation</div>
    <p class="express-desc">Desktop app and engine — ready in minutes.</p>
    <button
      type="button"
      class="btn-hero"
      disabled={busy}
      onclick={onExpressInstall}
    >
      Install
    </button>
  </div>

  <button type="button" class="customize-link" onclick={onCustomize}>
    Customize installation…
  </button>

  <p class="location-line">
    <span class="muted">Location:</span>
    <span title={bootstrap.installRoot}>{truncatePath(bootstrap.installRoot, 36)}</span>
    <button type="button" class="link" onclick={onPickLocation}>Change…</button>
  </p>

  {#if bootstrap.versionMismatch}
    <p class="banner-warn">An update is available for your installation.</p>
  {/if}
</section>

<style>
  .welcome {
    max-width: 520px;
    margin: 0 auto;
    padding-top: 0.5rem;
  }

  .hero {
    text-align: center;
    margin-bottom: 1.75rem;
  }

  .hero-mark {
    border-radius: 14px;
    margin-bottom: 1rem;
    box-shadow: 0 8px 32px rgb(131 68 245 / 0.2);
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
    margin: 0;
  }

  .express-card {
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 1.25rem;
    margin-bottom: 1rem;
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
    margin: 0 0 1rem;
  }

  .btn-hero {
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

  .btn-hero:hover:not(:disabled) {
    background: var(--installer-accent-hover);
  }

  .btn-hero:focus-visible {
    outline: 2px solid white;
    outline-offset: 2px;
  }

  .customize-link {
    display: block;
    width: 100%;
    text-align: center;
    background: none;
    border: none;
    color: var(--installer-accent);
    font-size: var(--installer-body-size);
    padding: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .location-line {
    text-align: center;
    font-size: var(--installer-caption-size);
    color: var(--installer-text-secondary);
    margin: 0;
  }

  .muted {
    color: var(--installer-muted);
  }

  .link {
    background: none;
    border: none;
    color: var(--installer-accent);
    margin-left: 0.35rem;
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
