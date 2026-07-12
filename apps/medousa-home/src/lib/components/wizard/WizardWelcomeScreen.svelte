<script lang="ts">
  import { onMount } from "svelte";
  import {
    Brain,
    ChevronDown,
    ChevronRight,
    LoaderCircle,
    Sparkles,
  } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import ProviderPicker from "$lib/components/settings/ProviderPicker.svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import {
    probeProviders,
    requireEngineReady,
    startEngine,
    validateProviderKey,
    waitForEngine,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";
  import { ensureSkipReadyModel } from "$lib/utils/wizardModelReady";
  import {
    ensureLocalModelReady,
    fetchLocalCatalog,
    fetchLocalHardware,
    formatBytes,
    loadLocalEngine,
    type LocalCatalogModel,
    type LocalCatalogResponse,
    type LocalHardwareResponse,
    type ModelDownloadProgress,
  } from "$lib/utils/localInferenceApi";
  import { layout } from "$lib/stores/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";

  type WizardPath = "byok" | "offline";

  let showAdvanced = $state(false);
  let selectedPath = $state<WizardPath | null>("offline");
  let byokProvider = $state("openai");
  let byokNeedsKey = $state(true);
  let apiKey = $state("");
  let baseUrl = $state("");
  let model = $state("gpt-5.4-mini");
  let probe = $state<ProvidersProbeResult | null>(null);
  let probing = $state(true);
  let validating = $state(false);
  let statusMessage = $state<string | null>(null);

  let localHardware = $state<LocalHardwareResponse | null>(null);
  let localCatalog = $state<LocalCatalogResponse | null>(null);
  let offlineModelId = $state<string | null>(null);
  let localLoading = $state(false);
  let downloadProgress = $state<ModelDownloadProgress | null>(null);

  const ollamaReady = $derived(probe?.ollamaDetected ?? false);
  const recommendedOfflineModel = $derived.by(() => {
    const catalog = localCatalog;
    if (!catalog) return null;
    return (
      catalog.models.find((entry) => entry.id === catalog.recommendedModelId) ??
      catalog.models.find((entry) => entry.tierRecommended) ??
      catalog.models[0] ??
      null
    );
  });

  onMount(() => {
    void refreshProbe();
    void refreshLocalInference();
  });

  async function refreshProbe() {
    probing = true;
    statusMessage = null;
    try {
      probe = await probeProviders();
      if (byokProvider === "ollama" && selectedPath === "byok") {
        model = probe.suggestedOllamaModel ?? "llama3.2";
      }
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      probing = false;
    }
  }

  async function refreshLocalInference() {
    localLoading = true;
    statusMessage = null;
    try {
      await startEngine({ privateBrain: true });
      const health = await waitForEngine(30);
      if (!health.ok) {
        statusMessage = health.message;
        return;
      }
      localHardware = await fetchLocalHardware();
      localCatalog = await fetchLocalCatalog();
      offlineModelId = localCatalog.recommendedModelId;
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localLoading = false;
    }
  }

  function selectPath(path: WizardPath) {
    selectedPath = path;
    statusMessage = null;
    downloadProgress = null;
    if (path === "byok") {
      showAdvanced = true;
      if (byokProvider === "ollama") {
        model = probe?.suggestedOllamaModel ?? "llama3.2";
      }
    }
    if (path === "offline") {
      void refreshLocalInference();
    }
  }

  function selectOfflineModel(entry: LocalCatalogModel) {
    offlineModelId = entry.id;
  }

  function onByokProviderChange(id: string, entry: ProviderCatalogEntry) {
    byokProvider = id;
    byokNeedsKey = entry.needsApiKey;
    statusMessage = null;
    model =
      id === "ollama"
        ? (probe?.suggestedOllamaModel ?? entry.defaultModel)
        : entry.defaultModel;
    baseUrl = entry.defaultBaseUrl ?? "";
  }

  function onPickerStatus(message: string | null, ok?: boolean) {
    if (message) {
      statusMessage = message;
    } else if (ok !== false) {
      statusMessage = null;
    }
  }

  async function skipSetup() {
    wizard.error = null;
    statusMessage = "Starting Medousa…";
    validating = true;
    try {
      await requireEngineReady({ privateBrain: false, timeoutSeconds: 45 });
      const modelReady = await ensureSkipReadyModel(
        wizard.existingProvider,
        wizard.existingModel,
        probe,
      );
      if (!modelReady.ok) {
        statusMessage = modelReady.message;
        wizard.error = modelReady.message;
        return;
      }
      await wizard.skipCurrent();
    } catch (err) {
      const message =
        err instanceof Error ? err.message : String(err);
      statusMessage =
        message || "Medousa engine did not start — try again or finish setup before continuing.";
      wizard.error = statusMessage;
    } finally {
      validating = false;
    }
  }

  async function continueOfflineSetup() {
    const modelId = offlineModelId ?? localCatalog?.recommendedModelId;
    if (!modelId) {
      statusMessage = "Pick a Gemma 4 model size first.";
      return;
    }

    if (localHardware && !localHardware.engineAvailable) {
      statusMessage =
        "Install the Offline brain package to use local Gemma on this computer.";
      return;
    }

    validating = true;
    statusMessage = "Starting the engine…";
    wizard.error = null;

    try {
      await startEngine({ privateBrain: true });
      const health = await waitForEngine(60);
      if (!health.ok) {
        statusMessage = health.message;
        return;
      }

      statusMessage = "Downloading Gemma 4 — this may take a while on first setup…";
      downloadProgress = await ensureLocalModelReady(modelId, (progress) => {
        downloadProgress = progress;
      });

      statusMessage = "Loading local brain…";
      const engine = await loadLocalEngine(modelId);
      if (!engine.loaded) {
        statusMessage = engine.message;
        return;
      }

      await wizard.applyScreen1Setup({
        path: "offline",
        provider: "medousa-local",
        model: modelId,
        baseUrl: engine.baseUrl,
        startCore: false,
      });
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      validating = false;
      downloadProgress = null;
    }
  }

  async function continueSetup() {
    if (!selectedPath) return;
    if (selectedPath === "offline") {
      await continueOfflineSetup();
      return;
    }

    validating = true;
    statusMessage = null;
    wizard.error = null;

    try {
      const provider = byokProvider;
      const validation = await validateProviderKey({
        provider,
        apiKey: byokNeedsKey ? apiKey : "",
        baseUrl: baseUrl.trim() || probe?.ollamaBaseUrl || null,
      });

      if (!validation.ok) {
        statusMessage = validation.message;
        return;
      }

      const resolvedModel = model.trim() || validation.suggestedModel || "gpt-5.4-mini";

      await wizard.applyScreen1Setup({
        path: selectedPath,
        provider,
        model: resolvedModel,
        baseUrl: baseUrl.trim() || (provider === "ollama" ? probe?.ollamaBaseUrl : null) || null,
        apiKey: byokNeedsKey && apiKey.trim() ? apiKey.trim() : null,
        startCore: true,
      });
    } catch {
      // wizard store sets error
    } finally {
      validating = false;
    }
  }

  const canContinue = $derived.by(() => {
    if (wizard.busy || validating) return false;
    if (!selectedPath) return false;
    if (selectedPath === "offline") {
      if (localLoading || probing) return false;
      return Boolean(
        localCatalog &&
          (offlineModelId ?? localCatalog.recommendedModelId),
      );
    }
    if (probing) return false;
    if (byokProvider === "ollama") {
      return ollamaReady && model.trim().length > 0;
    }
    if (!byokNeedsKey) {
      return model.trim().length > 0;
    }
    return apiKey.trim().length > 0 && model.trim().length > 0;
  });

  const continueLabel = $derived.by(() => {
    if (validating || wizard.busy) {
      if (selectedPath === "offline" && downloadProgress) {
        return `Downloading ${Math.round(downloadProgress.percent)}%`;
      }
      return "Starting Medousa…";
    }
    if (selectedPath === "offline") return "Download Gemma 4 & continue";
    return "Continue";
  });
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 1 of 2</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Welcome to Medousa
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Let's get you talking. Pick how Medousa thinks on this computer — you can change it later in
    Settings.
  </p>

  {#if probing || localLoading}
    <div class="mt-6 flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Checking this computer…
    </div>
  {/if}

  <button
    type="button"
    class="wizard-path-card mt-6 text-left {selectedPath === 'offline'
      ? 'wizard-path-card-active'
      : ''}"
    disabled={wizard.busy}
    onclick={() => selectPath("offline")}
  >
    <div class="flex items-start gap-3">
      <Sparkles class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <p class="font-semibold text-surface-50">Recommended — private on this computer</p>
        <p class="mt-1 text-sm text-surface-300">
          {#if localLoading}
            Finding the right local model for your hardware…
          {:else if localHardware && recommendedOfflineModel}
            We'll use
            <strong class="text-surface-100">{recommendedOfflineModel.displayName}</strong>
            (~{formatBytes(recommendedOfflineModel.sizeBytes)} download). Nothing leaves this
            device unless you choose cloud later.
          {:else if localHardware && !localHardware.engineAvailable}
            Offline brain is not installed — add it from Settings → Packages, or pick Advanced below.
          {:else}
            Download a local model once — chat without sending data to the cloud.
          {/if}
        </p>

        {#if selectedPath === "offline" && localHardware && !localHardware.engineAvailable}
          <div class="mt-4 border-t border-surface-500/30 pt-4">
            <button
              type="button"
              class="btn preset-filled-primary-500 w-full"
              disabled={wizard.busy || validating}
              onclick={(event) => {
                event.stopPropagation();
                settingsNav.openSection("packages");
                layout.navigateDesktop("settings", { bump: true });
              }}
            >
              Open Settings → Packages
            </button>
          </div>
        {:else if selectedPath === "offline" && localCatalog}
          <div class="mt-4 space-y-2 border-t border-surface-500/30 pt-4">
            {#each localCatalog.models as entry (entry.id)}
              <button
                type="button"
                class="settings-depth-card w-full text-left {(offlineModelId ?? localCatalog.recommendedModelId) === entry.id
                  ? 'settings-depth-card-active'
                  : ''}"
                disabled={wizard.busy || validating}
                onclick={(event) => {
                  event.stopPropagation();
                  selectOfflineModel(entry);
                }}
              >
                <span class="block text-sm font-medium text-surface-100">{entry.displayName}</span>
                <span class="workshop-faint mt-1 block text-xs">
                  ~{formatBytes(entry.sizeBytes)}
                  {#if entry.tierRecommended}
                    · recommended
                  {/if}
                </span>
              </button>
            {/each}
          </div>
        {/if}

        {#if downloadProgress && selectedPath === "offline"}
          <div class="mt-3">
            <div class="h-2 overflow-hidden rounded-full bg-surface-800">
              <div
                class="h-full rounded-full bg-primary-500 transition-all duration-300"
                style:width="{Math.max(4, Math.round(downloadProgress.percent))}%"
              ></div>
            </div>
            <p class="workshop-faint mt-2 text-xs">{downloadProgress.message}</p>
          </div>
        {/if}
      </div>
    </div>
  </button>

  <button
    type="button"
    class="workshop-text-action mt-4 inline-flex items-center gap-1 text-sm"
    onclick={() => (showAdvanced = !showAdvanced)}
  >
    {#if showAdvanced}
      <ChevronDown class="h-4 w-4" aria-hidden="true" />
    {:else}
      <ChevronRight class="h-4 w-4" aria-hidden="true" />
    {/if}
    Advanced — use your own API key
  </button>

  {#if showAdvanced}
    <div
      class="wizard-path-card mt-3 {selectedPath === 'byok' ? 'wizard-path-card-active' : ''}"
      role="group"
      aria-label="Use your own model provider"
    >
      <button
        type="button"
        class="w-full text-left"
        disabled={wizard.busy}
        onclick={() => selectPath("byok")}
      >
        <div class="flex items-start gap-3">
          <Brain class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
          <div class="min-w-0">
            <p class="font-semibold text-surface-50">Your API key or Ollama</p>
            <p class="mt-1 text-sm text-surface-300">
              OpenAI, Anthropic, DeepSeek, Groq, and 20+ more — or Ollama on this computer. Keys stay
              on this device.
            </p>
          </div>
        </div>
      </button>

      {#if selectedPath === "byok"}
        <div class="mt-4 border-t border-surface-500/30 pt-4">
          <ProviderPicker
            providerId={byokProvider}
            {model}
            {apiKey}
            {baseUrl}
            disabled={wizard.busy || validating}
            excludeProviderIds={["medousa-local"]}
            showValidate={false}
            onProviderChange={onByokProviderChange}
            onModelChange={(value) => (model = value)}
            onApiKeyChange={(value) => (apiKey = value)}
            onBaseUrlChange={(value) => (baseUrl = value)}
            onStatus={onPickerStatus}
          />
        </div>
      {/if}
    </div>
  {/if}

  {#if statusMessage}
    <p class="mt-4 text-sm text-warning-200">{statusMessage}</p>
  {/if}

  {#if wizard.existingProvider && !selectedPath}
    <p class="workshop-faint mt-4 text-xs">
      Current setup: {wizard.existingProvider}
      {#if wizard.existingModel}
        · {wizard.existingModel}
      {/if}
    </p>
  {/if}

  <div class="mt-auto flex flex-wrap items-center justify-between gap-3 pt-8">
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="btn variant-ghost min-h-11"
        disabled={wizard.busy || validating}
        onclick={() => void skipSetup()}
      >
        Skip for now
      </button>
      <button
        type="button"
        class="btn variant-ghost min-h-11"
        disabled={wizard.busy || probing || localLoading || validating}
        onclick={() => {
          void refreshProbe();
          if (selectedPath === "offline") void refreshLocalInference();
        }}
      >
        Try again
      </button>
    </div>
    <button
      type="button"
      class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
      disabled={!canContinue}
      onclick={() => void continueSetup()}
    >
      {#if validating || wizard.busy}
        <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
        {continueLabel}
      {:else}
        {continueLabel}
        <ChevronRight class="h-4 w-4" aria-hidden="true" />
      {/if}
    </button>
  </div>
</div>

<style>
  .wizard-path-card {
    display: block;
    width: 100%;
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-950) / 0.4);
    padding: 1.25rem;
    text-align: left;
    transition:
      border-color 150ms ease,
      background 150ms ease;
  }

  .wizard-path-card:hover:not(:disabled) {
    border-color: rgb(var(--color-primary-500) / 0.35);
  }

  .wizard-path-card-active {
    border-color: rgb(var(--color-primary-500) / 0.55);
    background: rgb(var(--color-primary-500) / 0.08);
  }
</style>
