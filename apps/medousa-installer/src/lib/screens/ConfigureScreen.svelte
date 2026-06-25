<script lang="ts">
  import type { BootstrapResponse, ConfigureMode } from "../types";

  interface Props {
    bootstrap: BootstrapResponse;
    onNext: (mode: ConfigureMode) => void;
    onPickLocation: () => void;
  }

  let { bootstrap, onNext, onPickLocation }: Props = $props();
  let mode = $state<ConfigureMode>("express");
  $effect(() => {
    if (bootstrap.modifyMode && mode === "express") {
      mode = "existing";
    }
  });
</script>

<section class="screen">
  <header class="screen-header">
    <h1>Install Medousa</h1>
    <p class="lead">Select how you want to configure your installation.</p>
  </header>

  <div class="option-list">
    <label class="option {mode === 'express' ? 'selected' : ''}">
      <input type="radio" name="configure" value="express" bind:group={mode} />
      <div>
        <div class="option-title">Express</div>
        <div class="option-desc">Desktop app and engine — recommended for most people.</div>
      </div>
    </label>

    {#if bootstrap.modifyMode}
      <label class="option {mode === 'existing' ? 'selected' : ''}">
        <input type="radio" name="configure" value="existing" bind:group={mode} />
        <div>
          <div class="option-title">Modify existing installation</div>
          <div class="option-desc">Add, update, or remove packages from your current install.</div>
        </div>
      </label>
    {/if}

    <label class="option {mode === 'manual' ? 'selected' : ''}">
      <input type="radio" name="configure" value="manual" bind:group={mode} />
      <div>
        <div class="option-title">Select workloads and components manually</div>
        <div class="option-desc">Pick workloads, adapters, offline brain, and model packs individually.</div>
      </div>
    </label>
  </div>

  <div class="location-row card">
    <div>
      <div class="muted">Install location</div>
      <div class="location-path">{bootstrap.installRoot}</div>
    </div>
    <button class="link-btn" type="button" onclick={onPickLocation}>Change…</button>
  </div>

  {#if bootstrap.versionMismatch}
    <p class="warning">Installed version {bootstrap.installedVersion} differs from release {bootstrap.remoteVersion}.</p>
  {/if}

  <footer class="footer-actions">
    <button class="primary" type="button" onclick={() => onNext(mode)}>Next</button>
  </footer>
</section>
