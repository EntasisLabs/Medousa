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
  import { isTauriIos } from "$lib/platform";

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
  let downloadingModelId = $state<string | null>(null);
  let customModelId = $state("");

  const engineReady = $derived(Boolean(engineStatus?.loaded));
  const activeModelId = $derived(engineStatus?.modelAlias ?? null);
  const catalogModels = $derived(localCatalog?.models ?? []);
  const customInstalledModels = $derived(
    installedModels.filter(
      (entry) => !catalogModels.some((model) => model.id === entry.modelId),
    ),
  );

  const statusMeta = $derived.by(() => {
    if (localBusy && !engineStatus && !localHardware) return "Checking hardware…";
    if (engineReady) {
      return engineStatus?.modelAlias
        ? `Ready · ${engineStatus.modelAlias}`
        : "Ready · private model loaded";
    }
    if (localHardware) {
      return `Idle · ${localHardware.profile.tierLabel} · ${localHardware.profile.recommendedDisplayName}`;
    }
    return isTauriIos()
      ? "Optional private models on this device"
      : `Optional private models ${onThisHostPhrase()}`;
  });

  onMount(() => {
    void refreshLocalPanel();
  });

  async function refreshLocalPanel() {
    localBusy = true;
    localMessage = null;
    try {
      if (!isTauriIos()) {
        // Desktop probes the sidecar. iOS talks directly to the in-process MLX runtime.
        await startEngine({ privateBrain: false });
        const health = await waitForEngine(20);
        if (!health.ok) {
          localMessage = health.message;
          return;
        }
      }
      const [hardware, catalog, models, status] = await Promise.all([
        fetchLocalHardware(),
        fetchLocalCatalog(),
        fetchLocalModels(),
        fetchLocalEngineStatus(),
      ]);
      localHardware = hardware;
      localCatalog = catalog;
      installedModels = models.installed;
      engineStatus = status;
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
    }
  }

  async function downloadModel(modelId: string, label = modelId) {
    const normalized = modelId.trim();
    if (!normalized) return;
    localBusy = true;
    downloadingModelId = normalized;
    localMessage = `Downloading ${label}…`;
    try {
      downloadProgress = await ensureLocalModelReady(normalized, (progress) => {
        downloadProgress = progress;
      });
      await refreshLocalPanel();
      localMessage = `${label} is ready.`;
      customModelId = "";
    } catch (err) {
      localMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localBusy = false;
      downloadProgress = null;
      downloadingModelId = null;
    }
  }

  function installedModel(modelId: string): InstalledLocalModel | null {
    return installedModels.find((entry) => entry.modelId === modelId) ?? null;
  }

  function catalogMeta(model: LocalCatalogResponse["models"][number]): string {
    const bits = [model.variant, formatBytes(model.sizeBytes)];
    if (model.modalities.includes("image")) bits.push("Vision");
    if (model.tierRecommended) bits.unshift("Recommended");
    return bits.join(" · ");
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
        <span class="brain-title">On-device models</span>
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

    {#each catalogModels as model (model.id)}
      {@const installed = installedModel(model.id)}
      <div class="brain-tile">
        <span class="brain-copy">
          <span class="brain-title">{model.displayName}</span>
          <span class="brain-meta">{catalogMeta(model)}</span>
        </span>
        {#if installed}
          <button
            type="button"
            class="brain-cta"
            disabled={disabled || localBusy || activeModelId === model.id}
            onclick={() => void loadEngine(model.id)}
          >
            {activeModelId === model.id ? "Loaded" : "Load"}
          </button>
          <button
            type="button"
            class="brain-cta brain-cta-danger"
            disabled={disabled || localBusy}
            aria-label="Remove {model.displayName}"
            onclick={() => void removeModel(model.id)}
          >
            <Trash2 size={14} strokeWidth={1.75} />
          </button>
        {:else}
          <button
            type="button"
            class="brain-cta"
            disabled={disabled || localBusy}
            onclick={() => void downloadModel(model.id, model.displayName)}
          >
            {#if downloadingModelId === model.id}
              <LoaderCircle size={14} strokeWidth={1.75} class="brain-spin" aria-hidden="true" />
            {:else}
              <Download size={14} strokeWidth={1.75} aria-hidden="true" />
            {/if}
            Download
          </button>
        {/if}
      </div>
    {/each}

    {#each customInstalledModels as entry (entry.modelId)}
      <div class="brain-tile">
        <span class="brain-copy">
          <span class="brain-title">{entry.modelId}</span>
          <span class="brain-meta">
            Custom MLX checkpoint · {formatBytes(entry.bytesOnDisk)}
          </span>
        </span>
        <button
          type="button"
          class="brain-cta"
          disabled={disabled || localBusy || activeModelId === entry.modelId}
          onclick={() => void loadEngine(entry.modelId)}
        >
          {activeModelId === entry.modelId ? "Loaded" : "Load"}
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

    {#if isTauriIos()}
      <form
        class="brain-custom"
        onsubmit={(event) => {
          event.preventDefault();
          void downloadModel(customModelId, customModelId.trim());
        }}
      >
        <label class="brain-copy" for="brain-custom-model">
          <span class="brain-title">Other MLX model</span>
          <span class="brain-meta">Enter a full Hugging Face repository ID.</span>
        </label>
        <div class="brain-custom-controls">
          <input
            id="brain-custom-model"
            class="brain-custom-input"
            type="text"
            placeholder="mlx-community/model-name"
            bind:value={customModelId}
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            disabled={disabled || localBusy}
          />
          <button
            type="submit"
            class="brain-cta"
            disabled={disabled || localBusy || !customModelId.trim()}
          >
            Download
          </button>
        </div>
      </form>
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

  .brain-custom {
    display: grid;
    gap: 0.55rem;
    padding: 0.7rem 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.2);
  }

  .brain-custom-controls {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }

  .brain-custom-input {
    min-width: 0;
    flex: 1 1 auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-950) / 0.35);
    padding: 0.5rem 0.6rem;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-100));
  }

  .brain-custom-input:focus {
    border-color: rgb(var(--color-primary-500) / 0.55);
    outline: none;
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
