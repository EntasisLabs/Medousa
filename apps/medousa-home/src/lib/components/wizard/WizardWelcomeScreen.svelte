<script lang="ts">
  import { onDestroy, onMount } from "svelte";
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
    startEngine,
    validateProviderKey,
    waitForEngine,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";
  import {
    fetchLocalCatalog,
    fetchLocalHardware,
    formatBytes,
    type LocalCatalogModel,
    type LocalCatalogResponse,
    type LocalHardwareResponse,
  } from "$lib/utils/localInferenceApi";
  import {
    installPackage,
    listenPackageProgress,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import { layout } from "$lib/stores/layout.svelte";
  import { hostComputerPhrase } from "$lib/platformCopy";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { isTauri } from "$lib/window";

  type WizardPath = "byok" | "offline";

  const hostPhrase = hostComputerPhrase();
  const LOCAL_BRAIN_PACKAGE = "local-brain";

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
  let installingBrain = $state(false);
  let packageProgress = $state<PackageProgressEvent | null>(null);
  let unlistenPackage: (() => void) | null = null;

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
    if (isTauri()) {
      void listenPackageProgress((event) => {
        if (event.packageId !== LOCAL_BRAIN_PACKAGE) return;
        packageProgress = event;
      }).then((fn) => {
        unlistenPackage = fn;
      });
    }
  });

  onDestroy(() => {
    unlistenPackage?.();
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

  async function refreshLocalInference(options?: { startDownload?: boolean }) {
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
      if (
        options?.startDownload !== false &&
        localHardware.engineAvailable &&
        offlineModelId
      ) {
        wizard.beginBrainModelPrep(offlineModelId);
      }
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      localLoading = false;
    }
  }

  function selectPath(path: WizardPath) {
    selectedPath = path;
    statusMessage = null;
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
    if (localHardware?.engineAvailable) {
      wizard.beginBrainModelPrep(entry.id);
    }
  }

  async function installOfflineBrain() {
    if (!isTauri() || installingBrain) return;
    installingBrain = true;
    statusMessage = null;
    packageProgress = {
      packageId: LOCAL_BRAIN_PACKAGE,
      displayName: "Offline brain",
      phase: "downloading",
      phaseLabel: "Downloading",
      percent: 0,
      message: "Starting Offline brain install…",
    };
    try {
      await installPackage(LOCAL_BRAIN_PACKAGE);
      await refreshLocalInference({ startDownload: true });
      if (localHardware?.engineAvailable) {
        statusMessage = null;
      } else {
        statusMessage =
          "Offline brain installed — if models don’t appear, try again or open Settings → Packages.";
      }
    } catch (err) {
      statusMessage =
        err instanceof Error
          ? err.message
          : "Couldn’t install Offline brain — try again or Settings → Packages.";
    } finally {
      installingBrain = false;
    }
  }

  function onByokProviderChange(id: string, entry: ProviderCatalogEntry) {
    byokProvider = id;
    byokNeedsKey = entry.needsApiKey;
    statusMessage = null;
    // Model id comes from ProviderPicker via onModelChange after catalog/live resolve.
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
    statusMessage = null;
    validating = true;
    try {
      // Brain is optional even on the AI branch — skip without forcing a model.
      await wizard.skipBrain();
    } catch (err) {
      const message =
        err instanceof Error ? err.message : String(err);
      statusMessage = message || "Could not continue — try again.";
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
      statusMessage = "Install Offline brain first — download can finish in the background.";
      return;
    }

    validating = true;
    statusMessage = null;
    wizard.error = null;

    try {
      await startEngine({ privateBrain: true });
      const health = await waitForEngine(60);
      if (!health.ok) {
        statusMessage = health.message;
        return;
      }

      // Persist choice + advance; model download / engine load continue in the store.
      wizard.beginBrainModelPrep(modelId);
      await wizard.applyScreen1Setup({
        path: "offline",
        provider: "medousa-local",
        model: modelId,
        baseUrl: null,
        startCore: false,
      });
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      validating = false;
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
    if (wizard.busy || validating || installingBrain) return false;
    if (!selectedPath) return false;
    if (selectedPath === "offline") {
      if (localLoading || probing) return false;
      if (!localHardware?.engineAvailable) return false;
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
    if (validating || wizard.busy) return "Working…";
    if (selectedPath === "offline") return "Continue";
    return "Continue";
  });

  const brainProgress = $derived(wizard.brainDownloadProgress);
</script>

<div class="flex h-full flex-col">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <h1 id="product-wizard-title" class="mt-4 text-2xl font-semibold text-surface-50">
    Give this desk a brain
  </h1>
  <p class="mt-2 text-sm text-surface-400">
    Private on {hostPhrase}, or your own key. Skip anytime — your desk still opens.
  </p>

  {#if probing || localLoading}
    <div class="mt-6 flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Checking hardware…
    </div>
  {/if}

  <div
    class="wizard-path-card mt-6 {selectedPath === 'offline'
      ? 'wizard-path-card-active'
      : ''}"
    role="group"
    aria-label="Recommended — private"
  >
    <button
      type="button"
      class="wizard-path-card-select w-full text-left"
      disabled={wizard.busy}
      onclick={() => selectPath("offline")}
    >
      <div class="flex items-start gap-3">
        <Sparkles class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <p class="font-semibold text-surface-50">Recommended — private</p>
          <p class="mt-1 text-sm text-surface-300">
            {#if localLoading}
              Finding the right local model for your hardware…
            {:else if localHardware && recommendedOfflineModel}
              We'll use
              <strong class="text-surface-100">{recommendedOfflineModel.displayName}</strong>
              (~{formatBytes(recommendedOfflineModel.sizeBytes)} download). Nothing leaves this
              device unless you choose cloud later.
            {:else if localHardware && !localHardware.engineAvailable}
              Install Offline brain here — then pick a model size. Download keeps going if you Continue.
            {:else}
              Download a local model once — chat without sending data to the cloud. You can Continue while it finishes.
            {/if}
          </p>
        </div>
      </div>
    </button>

    {#if selectedPath === "offline" && localHardware && !localHardware.engineAvailable}
      <div class="mt-4 space-y-3 border-t border-surface-500/30 px-4 pb-4 pt-4">
        <button
          type="button"
          class="btn preset-filled-primary-500 inline-flex w-full items-center justify-center gap-2"
          disabled={wizard.busy || validating || installingBrain}
          onclick={() => void installOfflineBrain()}
        >
          {#if installingBrain}
            <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
            {packageProgress
              ? `${packageProgress.phaseLabel} ${Math.round(packageProgress.percent)}%`
              : "Installing Offline brain…"}
          {:else}
            Install Offline brain
          {/if}
        </button>
        {#if packageProgress && installingBrain}
          <div class="h-2 overflow-hidden rounded-full bg-surface-800">
            <div
              class="h-full rounded-full bg-primary-500 transition-all duration-300"
              style:width="{Math.max(4, Math.round(packageProgress.percent))}%"
            ></div>
          </div>
          <p class="workshop-faint text-xs">{packageProgress.message}</p>
        {/if}
        <button
          type="button"
          class="workshop-text-action text-xs"
          disabled={wizard.busy || installingBrain}
          onclick={() => {
            settingsNav.openSection("packages");
            layout.navigateDesktop("settings", { bump: true });
          }}
        >
          Advanced — Settings → Packages
        </button>
      </div>
    {:else if selectedPath === "offline" && localCatalog}
      <div class="space-y-2 border-t border-surface-500/30 px-4 pb-4 pt-4">
        {#each localCatalog.models as entry (entry.id)}
          <button
            type="button"
            class="settings-depth-card w-full text-left {(offlineModelId ?? localCatalog.recommendedModelId) === entry.id
              ? 'settings-depth-card-active'
              : ''}"
            disabled={wizard.busy || validating || installingBrain}
            onclick={() => selectOfflineModel(entry)}
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

    {#if selectedPath === "offline" && brainProgress && brainProgress.phase !== "ready"}
      <div class="px-4 pb-4">
        <div class="h-2 overflow-hidden rounded-full bg-surface-800">
          <div
            class="h-full rounded-full bg-primary-500 transition-all duration-300"
            style:width="{Math.max(4, Math.round(brainProgress.percent))}%"
          ></div>
        </div>
        <p class="workshop-faint mt-2 text-xs">
          {brainProgress.message}
          {#if brainProgress.phase !== "failed"}
            · continues in the background if you Continue
          {/if}
        </p>
      </div>
    {/if}
  </div>

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
              OpenAI, Anthropic, DeepSeek, Groq, and 20+ more — or Ollama. Keys stay on this device.
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
        disabled={wizard.busy || probing || localLoading || validating || installingBrain}
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

  .wizard-path-card-select {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    cursor: pointer;
  }

  .wizard-path-card-select:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }
</style>
