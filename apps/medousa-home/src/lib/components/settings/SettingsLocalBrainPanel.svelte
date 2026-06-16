<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, Trash2 } from "@lucide/svelte";
  import {
    ensureLocalModelReady,
    fetchLocalCatalog,
    fetchLocalEngineStatus,
    fetchLocalHardware,
    fetchLocalModels,
    formatBytes,
    loadLocalEngine,
    removeLocalModel,
    type InstalledLocalModel,
    type LocalCatalogResponse,
    type LocalEngineStatus,
    type LocalHardwareResponse,
    type ModelDownloadProgress,
  } from "$lib/utils/localInferenceApi";
  import { startEngine, waitForEngine } from "$lib/utils/providersApi";
  import { localBrainOnDeviceHint } from "$lib/platformCopy";

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

  onMount(() => {
    void refreshLocalPanel();
  });

  async function refreshLocalPanel() {
    localBusy = true;
    localMessage = null;
    try {
      await startEngine({ privateBrain: true });
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

<article class="settings-profile-card mt-6">
  <header class="settings-profile-header">
    <div class="min-w-0">
      <h3 class="settings-profile-title">Private brain</h3>
      <p class="settings-profile-subtitle">
        {localBrainOnDeviceHint()}
      </p>
    </div>
    <span
      class="settings-profile-status {engineReady
        ? 'settings-profile-status-ok'
        : 'settings-profile-status-warn'}"
    >
      {engineReady ? "Ready" : "Idle"}
    </span>
  </header>

  {#if localHardware}
    <p class="settings-profile-detail">
      Tier {localHardware.profile.tierLabel} · recommended {localHardware.profile.recommendedDisplayName}
    </p>
  {/if}

  {#if engineStatus}
    <p class="settings-profile-detail">
      Engine {engineStatus.loaded ? "running" : "idle"}
      {#if engineStatus.modelAlias}
        · {engineStatus.modelAlias}
      {/if}
    </p>
  {/if}

  {#if downloadProgress}
    <div class="mt-4">
      <div class="settings-profile-progress-track">
        <div
          class="settings-profile-progress-fill"
          style:width="{Math.max(4, Math.round(downloadProgress.percent))}%"
        ></div>
      </div>
      <p class="settings-profile-detail mt-2">{downloadProgress.message}</p>
    </div>
  {/if}

  <div class="settings-profile-actions mt-4 flex flex-wrap gap-2">
    <button
      type="button"
      class="btn variant-soft-primary min-h-9 text-sm"
      disabled={disabled || localBusy || !recommendedModelId}
      onclick={() => void downloadRecommended()}
    >
      Download recommended Gemma 4
    </button>
    <button
      type="button"
      class="btn variant-ghost min-h-9 text-sm"
      disabled={disabled || localBusy}
      onclick={() => void refreshLocalPanel()}
    >
      {#if localBusy}
        <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
      {/if}
      Re-probe hardware
    </button>
  </div>

  {#if installedModels.length > 0}
    <ul class="mt-4 space-y-2">
      {#each installedModels as entry (entry.modelId)}
        <li class="settings-profile-list-row">
          <div class="min-w-0">
            <p class="text-sm font-medium text-surface-100">{entry.modelId}</p>
            <p class="settings-profile-detail">
              {formatBytes(entry.bytesOnDisk)} on disk · {entry.verified ? "verified" : "pending"}
            </p>
          </div>
          <div class="flex gap-2">
            <button
              type="button"
              class="btn variant-ghost min-h-9 text-sm"
              disabled={disabled || localBusy}
              onclick={() => void loadEngine(entry.modelId)}
            >
              Load
            </button>
            <button
              type="button"
              class="btn variant-ghost min-h-9 text-sm text-warning-200"
              disabled={disabled || localBusy}
              onclick={() => void removeModel(entry.modelId)}
            >
              <Trash2 class="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {:else if localHardware?.engineAvailable}
    <p class="settings-profile-detail mt-4">No local models installed yet.</p>
  {/if}

  {#if localMessage}
    <p class="settings-inline-status mt-4">{localMessage}</p>
  {/if}
</article>
