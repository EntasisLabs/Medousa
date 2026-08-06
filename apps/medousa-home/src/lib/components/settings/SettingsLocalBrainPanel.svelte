<script lang="ts">
  import { onMount } from "svelte";
  import { Download, LoaderCircle, Power, RefreshCw, Trash2 } from "@lucide/svelte";
  import {
    ensureLocalModelReady,
    fetchLocalCatalog,
    fetchLocalEngineStatus,
    fetchLocalHardware,
    fetchLocalModels,
    formatBytes,
    loadLocalEngine,
    removeLocalModel,
    unloadLocalEngine,
    type InstalledLocalModel,
    type LocalCatalogResponse,
    type LocalEngineStatus,
    type LocalHardwareResponse,
    type ModelDownloadProgress,
  } from "$lib/utils/localInferenceApi";
  import { startEngine, waitForEngine } from "$lib/utils/providersApi";
  import { localBrainOnDeviceHint, onThisHostPhrase } from "$lib/platformCopy";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  let localHardware = $state<LocalHardwareResponse | null>(null);
  let localCatalog = $state<LocalCatalogResponse | null>(null);
  let installedModels = $state<InstalledLocalModel[]>([]);
  let engineStatus = $state<LocalEngineStatus | null>(null);
  let localBusy = $state(false);
  let localMessage = $state<string | null>(null);
  let downloadProgress = $state<ModelDownloadProgress | null>(null);

  const recommendedModelId = $derived(localCatalog?.recommendedModelId ?? null);
  const engineReady = $derived(Boolean(engineStatus?.loaded));

  const statusMeta = $derived.by(() => {
    if (localBusy && !engineStatus && !localHardware) return "Checking hardware…";
    if (engineReady) {
      return engineStatus?.modelAlias
        ? `Ready · ${engineStatus.modelAlias}`
        : "Ready · offline Gemma loaded";
    }
    if (localHardware) {
      return `Idle · ${localHardware.profile.tierLabel} · ${localHardware.profile.recommendedDisplayName}`;
    }
    return `Optional offline Gemma ${onThisHostPhrase()}`;
  });

  onMount(() => {
    void refreshLocalPanel();
  });

  async function refreshLocalPanel() {
    localBusy = true;
    localMessage = null;
    try {
      // Probe only — never auto-spawn the offline brain (Load does that).
      await startEngine({ privateBrain: false });
      const health = await waitForEngine(20);
      if (!health.ok) {
        localMessage = health.message;
        return;
      }
      localHardware = await fetchLocalHardware();
      localCatalog = await fetchLocalCatalog();
      const models = await fetchLocalModels();
      installedModels = models.installed;
      engineStatus = await fetchLocalEngineStatus();
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
    }
  }

  async function downloadRecommended() {
    if (!recommendedModelId) return;
    localBusy = true;
    localMessage = "Downloading recommended Gemma 4 model…";
    try {
      downloadProgress = await ensureLocalModelReady(recommendedModelId);
      await refreshLocalPanel();
      localMessage = "Download complete.";
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
      downloadProgress = null;
    }
  }

  async function loadEngine(modelId: string) {
    localBusy = true;
    localMessage = "Loading local engine…";
    try {
      engineStatus = await loadLocalEngine(modelId);
      localMessage = engineStatus.message;
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
    }
  }

  async function unloadEngine() {
    localBusy = true;
    localMessage = "Releasing local model memory…";
    try {
      engineStatus = await unloadLocalEngine();
      localMessage = "Local brain is cold. Model memory has been released.";
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
    }
  }

  async function removeModel(modelId: string) {
    localBusy = true;
    localMessage = null;
    try {
      await removeLocalModel(modelId);
      await refreshLocalPanel();
      localMessage = `Removed ${modelId}.`;
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
    }
  }
</script>

<div class="brain-band">
  <div class="brain-band-head">
    <h3 class="settings-subsection-heading">Private brain</h3>
    <p class="settings-subsection-lead">{localBrainOnDeviceHint()}</p>
  </div>

  <div class="brain-stack">
    <div class="brain-tile">
      <span class="brain-copy">
        <span class="brain-title">Offline Gemma</span>
        <span class="brain-meta">{statusMeta}</span>
      </span>
      <span class="brain-pill" class:brain-pill-ok={engineReady}>
        {engineReady ? "Ready" : "Idle"}
      </span>
      <span class="brain-actions">
        {#if engineReady}
          <button
            type="button"
            class="brain-icon-btn"
            disabled={disabled || localBusy}
            title="Unload and release model memory"
            aria-label="Unload local brain and release model memory"
            onclick={() => void unloadEngine()}
          >
            <Power size={15} strokeWidth={1.75} />
          </button>
        {/if}
        <button
          type="button"
          class="brain-icon-btn"
          disabled={disabled || localBusy || !recommendedModelId}
          title="Download recommended Gemma 4"
          aria-label="Download recommended Gemma 4"
          onclick={() => void downloadRecommended()}
        >
          {#if localBusy && downloadProgress}
            <LoaderCircle size={15} strokeWidth={1.75} class="brain-spin" aria-hidden="true" />
          {:else}
            <Download size={15} strokeWidth={1.75} />
          {/if}
        </button>
        <button
          type="button"
          class="brain-icon-btn"
          disabled={disabled || localBusy}
          title="Re-probe hardware"
          aria-label="Re-probe hardware"
          onclick={() => void refreshLocalPanel()}
        >
          {#if localBusy && !downloadProgress}
            <LoaderCircle size={15} strokeWidth={1.75} class="brain-spin" aria-hidden="true" />
          {:else}
            <RefreshCw size={15} strokeWidth={1.75} />
          {/if}
        </button>
      </span>
    </div>

    {#if downloadProgress}
      <div class="brain-tile brain-tile-col">
        <div class="brain-progress-track">
          <div
            class="brain-progress-fill"
            style:width="{Math.max(4, Math.round(downloadProgress.percent))}%"
          ></div>
        </div>
        <span class="brain-meta">{downloadProgress.message}</span>
      </div>
    {/if}

    {#each installedModels as entry (entry.modelId)}
      <div class="brain-tile">
        <span class="brain-copy">
          <span class="brain-title">{entry.modelId}</span>
          <span class="brain-meta">
            {formatBytes(entry.bytesOnDisk)} · {entry.verified ? "verified" : "pending"}
          </span>
        </span>
        <button
          type="button"
          class="brain-cta"
          disabled={disabled || localBusy}
          onclick={() => void loadEngine(entry.modelId)}
        >
          Load now
        </button>
        <button
          type="button"
          class="brain-cta brain-cta-danger"
          disabled={disabled || localBusy}
          aria-label="Remove {entry.modelId}"
          onclick={() => void removeModel(entry.modelId)}
        >
          <Trash2 size={14} strokeWidth={1.75} />
        </button>
      </div>
    {/each}

    {#if installedModels.length === 0 && localHardware?.engineAvailable}
      <p class="brain-footnote">No local models installed yet.</p>
    {/if}

    {#if localMessage}
      <p class="brain-footnote brain-footnote-status">{localMessage}</p>
    {/if}
  </div>
</div>

<style>
  .brain-band {
    margin-top: 1.25rem;
  }

  .brain-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .brain-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .brain-stack {
    display: grid;
    gap: 0.5rem;
  }

  .brain-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .brain-tile-col {
    flex-direction: column;
    align-items: stretch;
    gap: 0.45rem;
    min-height: 0;
  }

  .brain-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .brain-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .brain-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .brain-pill {
    flex-shrink: 0;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--theme-warning));
  }

  .brain-pill-ok {
    color: rgb(var(--theme-success));
  }

  .brain-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
  }

  .brain-icon-btn {
    display: inline-flex;
    height: 1.85rem;
    width: 1.85rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.28);
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease,
      color 120ms ease;
  }

  .brain-icon-btn:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.5);
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .brain-icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .brain-cta {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.3rem;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .brain-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .brain-cta-danger {
    color: rgb(var(--theme-error) / 0.9);
  }

  .brain-progress-track {
    height: 0.35rem;
    overflow: hidden;
    border-radius: 999px;
    background: rgb(var(--color-surface-800) / 0.8);
  }

  .brain-progress-fill {
    height: 100%;
    border-radius: inherit;
    background: rgb(var(--color-primary-500) / 0.85);
    transition: width 160ms ease;
  }

  .brain-footnote {
    margin: 0;
    padding: 0 0.15rem;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .brain-footnote-status {
    color: rgb(var(--theme-text-tertiary));
  }

  :global(.brain-spin) {
    animation: brain-spin 0.8s linear infinite;
  }

  @keyframes brain-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
