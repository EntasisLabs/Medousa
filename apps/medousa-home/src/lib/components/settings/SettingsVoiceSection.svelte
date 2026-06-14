<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, Trash2 } from "@lucide/svelte";
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import ProviderPicker from "$lib/components/settings/ProviderPicker.svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import { defaultSttModel } from "$lib/types/workshopDefaults";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
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

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let localHardware = $state<LocalHardwareResponse | null>(null);
  let localCatalog = $state<LocalCatalogResponse | null>(null);
  let installedModels = $state<InstalledLocalModel[]>([]);
  let engineStatus = $state<LocalEngineStatus | null>(null);
  let localBusy = $state(false);
  let localMessage = $state<string | null>(null);
  let downloadProgress = $state<ModelDownloadProgress | null>(null);
  let providerStatus = $state<string | null>(null);
  let sttProviderStatus = $state<string | null>(null);

  const readOnly = $derived(mobile && isTauriMobilePlatform());
  const recommendedModelId = $derived(localCatalog?.recommendedModelId ?? null);

  onMount(() => {
    if (!readOnly) void refreshLocalPanel();
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

  function onProviderChange(id: string, entry: ProviderCatalogEntry) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      provider: id,
      model: entry.defaultModel,
      baseUrl: entry.defaultBaseUrl,
    };
    providerStatus = null;
  }

  function onSttProviderChange(id: string, entry: ProviderCatalogEntry) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      sttProvider: id,
      sttModel: defaultSttModel(id),
      sttBaseUrl: entry.defaultBaseUrl,
    };
    sttProviderStatus = null;
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Voice</h2>
    <p class="workshop-faint mt-1 text-sm">
      Speech input for dictation, plus how chat answers are shaped.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="text-sm font-semibold text-surface-50">Speech input</h3>
    <p class="workshop-faint mt-1 text-xs">
      Transcribes the mic button in chat. Separate from your chat model — use any
      OpenAI-compatible Whisper endpoint (OpenAI, Groq, etc.).
    </p>
    <div class="mt-4">
      <ProviderPicker
        providerId={workshopDefaults.draft.sttProvider ?? "openai"}
        model={workshopDefaults.draft.sttModel ?? defaultSttModel(workshopDefaults.draft.sttProvider ?? "openai")}
        apiKey={workshopDefaults.sttApiKeyDraft}
        baseUrl={workshopDefaults.draft.sttBaseUrl ?? ""}
        disabled={readOnly || workshopDefaults.saving}
        excludeProviderIds={["medousa-local", "ollama"]}
        onProviderChange={onSttProviderChange}
        onModelChange={(value) =>
          (workshopDefaults.draft = { ...workshopDefaults.draft, sttModel: value })}
        onApiKeyChange={(value) => (workshopDefaults.sttApiKeyDraft = value)}
        onBaseUrlChange={(value) =>
          (workshopDefaults.draft = { ...workshopDefaults.draft, sttBaseUrl: value })}
        onStatus={(message, ok) => {
          sttProviderStatus = message;
          if (ok === true) sttProviderStatus = message;
        }}
      />
    </div>
    {#if sttProviderStatus}
      <p class="mt-2 text-xs text-surface-300">{sttProviderStatus}</p>
    {/if}
    {#if workshopDefaults.sttApiKeySet && !workshopDefaults.sttApiKeyDraft.trim()}
      <p class="workshop-faint mt-2 text-xs">
        A speech input API key is stored. Leave blank to keep it, or enter a new one to replace it.
      </p>
    {:else if workshopDefaults.apiKeySet && !workshopDefaults.sttApiKeySet}
      <p class="workshop-faint mt-2 text-xs">
        No speech key yet — your chat API key will be used as a fallback.
      </p>
    {/if}
  </div>

  <div class="mt-8 border-t border-surface-500/35 pt-6">
    <span class="block text-sm font-medium text-surface-100">Response depth</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      Applies to chat turns — shared with the TUI and CLI.
    </span>
    <div class="mt-3 grid gap-2 sm:grid-cols-3">
      {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.responseDepthMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          onclick={() =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              responseDepthMode: option.id,
            })}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <h3 class="text-sm font-semibold text-surface-50">Chat model</h3>
    <p class="workshop-faint mt-1 text-xs">
      Default provider and model for chat turns — shared with the TUI and CLI.
    </p>
    <div class="mt-4">
      <ProviderPicker
        providerId={workshopDefaults.draft.provider ?? "openai"}
        model={workshopDefaults.draft.model ?? ""}
        apiKey={workshopDefaults.apiKeyDraft}
        baseUrl={workshopDefaults.draft.baseUrl ?? ""}
        disabled={readOnly || workshopDefaults.saving}
        excludeProviderIds={["medousa-local"]}
        onProviderChange={onProviderChange}
        onModelChange={(value) =>
          (workshopDefaults.draft = { ...workshopDefaults.draft, model: value })}
        onApiKeyChange={(value) => (workshopDefaults.apiKeyDraft = value)}
        onBaseUrlChange={(value) =>
          (workshopDefaults.draft = { ...workshopDefaults.draft, baseUrl: value })}
        onStatus={(message, ok) => {
          providerStatus = message;
          if (ok === true) providerStatus = message;
        }}
      />
    </div>
    {#if providerStatus}
      <p class="mt-2 text-xs text-surface-300">{providerStatus}</p>
    {/if}
    {#if workshopDefaults.apiKeySet && !workshopDefaults.apiKeyDraft.trim()}
      <p class="workshop-faint mt-2 text-xs">An API key is already stored — enter a new one to replace it.</p>
    {/if}
  </div>

  {#if !readOnly}
    <div class="mt-8 border-t border-surface-500/35 pt-6">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 class="text-sm font-semibold text-surface-50">Local Gemma brain</h3>
          <p class="workshop-faint mt-1 text-xs">
            Your private brain runs on this Mac through Medousa Engine — no Ollama required.
          </p>
        </div>
        <button
          type="button"
          class="btn variant-ghost min-h-9 text-sm"
          disabled={localBusy}
          onclick={() => void refreshLocalPanel()}
        >
          {#if localBusy}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
          {/if}
          Re-probe hardware
        </button>
      </div>

      {#if localHardware}
        <p class="mt-4 text-sm text-surface-200">
          Tier <span class="font-medium">{localHardware.profile.tierLabel}</span>
          ({localHardware.profile.tier}) — recommended
          <span class="font-medium">{localHardware.profile.recommendedDisplayName}</span>
        </p>
      {/if}

      {#if engineStatus}
        <p class="workshop-faint mt-2 text-xs">
          Engine: {engineStatus.loaded ? "ready" : "idle"}
          {#if engineStatus.modelAlias}
            · {engineStatus.modelAlias}
          {/if}
          · {engineStatus.baseUrl}
        </p>
      {/if}

      {#if downloadProgress}
        <div class="mt-4">
          <div class="h-2 overflow-hidden rounded-full bg-surface-800">
            <div
              class="h-full rounded-full bg-primary-500 transition-all duration-300"
              style:width="{Math.max(4, Math.round(downloadProgress.percent))}%"
            ></div>
          </div>
          <p class="workshop-faint mt-2 text-xs">{downloadProgress.message}</p>
        </div>
      {/if}

      <div class="mt-4 flex flex-wrap gap-2">
        <button
          type="button"
          class="btn variant-soft-primary min-h-9 text-sm"
          disabled={localBusy || !recommendedModelId}
          onclick={() => void downloadRecommended()}
        >
          Download recommended Gemma 4
        </button>
      </div>

      {#if installedModels.length > 0}
        <ul class="mt-4 space-y-2">
          {#each installedModels as entry (entry.modelId)}
            <li class="settings-depth-card flex flex-wrap items-center justify-between gap-3">
              <div>
                <p class="text-sm font-medium text-surface-100">{entry.modelId}</p>
                <p class="workshop-faint text-xs">
                  {formatBytes(entry.bytesOnDisk)} on disk · {entry.verified ? "verified" : "pending"}
                </p>
              </div>
              <div class="flex gap-2">
                <button
                  type="button"
                  class="btn variant-ghost min-h-9 text-sm"
                  disabled={localBusy}
                  onclick={() => void loadEngine(entry.modelId)}
                >
                  Load
                </button>
                <button
                  type="button"
                  class="btn variant-ghost min-h-9 text-sm text-warning-200"
                  disabled={localBusy}
                  onclick={() => void removeModel(entry.modelId)}
                >
                  <Trash2 class="h-4 w-4" aria-hidden="true" />
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {:else if localHardware?.engineAvailable}
        <p class="workshop-faint mt-4 text-sm">No local models installed yet.</p>
      {/if}

      {#if localMessage}
        <p class="mt-4 text-sm text-surface-300">{localMessage}</p>
      {/if}
    </div>
  {/if}

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
