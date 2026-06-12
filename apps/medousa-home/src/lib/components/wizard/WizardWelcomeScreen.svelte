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
  import {
    probeProviders,
    startEngine,
    validateProviderKey,
    waitForEngine,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";
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

  type WizardPath = "byok" | "offline";
  type ByokProvider = "openai" | "anthropic" | "google" | "ollama";

  const BYOK_PROVIDERS: { id: ByokProvider; label: string; keyHint: string }[] = [
    { id: "openai", label: "OpenAI", keyHint: "sk-…" },
    { id: "anthropic", label: "Anthropic", keyHint: "sk-ant-…" },
    { id: "google", label: "Google Gemini", keyHint: "AI…" },
    { id: "ollama", label: "Ollama (local)", keyHint: "" },
  ];

  let showAdvanced = $state(false);
  let selectedPath = $state<WizardPath | null>("offline");
  let byokProvider = $state<ByokProvider>("ollama");
  let apiKey = $state("");
  let model = $state("llama3.2");
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

  function selectByokProvider(provider: ByokProvider) {
    byokProvider = provider;
    statusMessage = null;
    if (provider === "ollama") {
      model = probe?.suggestedOllamaModel ?? "llama3.2";
    } else if (provider === "openai") {
      model = "gpt-4o-mini";
    } else if (provider === "anthropic") {
      model = "claude-3-7-sonnet-latest";
    } else {
      model = "gemini-2.5-pro";
    }
  }

  async function continueOfflineSetup() {
    const modelId = offlineModelId ?? localCatalog?.recommendedModelId;
    if (!modelId) {
      statusMessage = "Pick a Gemma 4 model size first.";
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
        apiKey: provider === "ollama" ? "" : apiKey,
        baseUrl: provider === "ollama" ? probe?.ollamaBaseUrl : null,
      });

      if (!validation.ok) {
        statusMessage = validation.message;
        return;
      }

      const resolvedModel = model.trim() || validation.suggestedModel || "llama3.2";

      await wizard.applyScreen1Setup({
        path: selectedPath,
        provider,
        model: resolvedModel,
        baseUrl: provider === "ollama" ? probe?.ollamaBaseUrl : null,
        apiKey: provider === "ollama" ? null : apiKey.trim(),
        startCore: true,
      });
    } catch {
      // wizard store sets error
    } finally {
      validating = false;
    }
  }

  const canContinue = $derived.by(() => {
    if (wizard.busy || validating || probing || localLoading) return false;
    if (!selectedPath) return false;
    if (selectedPath === "offline") {
      return Boolean(
        localCatalog &&
          localHardware?.engineAvailable &&
          (offlineModelId ?? localCatalog.recommendedModelId),
      );
    }
    if (byokProvider === "ollama") {
      return ollamaReady && model.trim().length > 0;
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
            Local models aren't available in this build yet. Use Advanced options below.
          {:else}
            Download a local model once — chat without sending data to the cloud.
          {/if}
        </p>

        {#if selectedPath === "offline" && localCatalog}
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
              OpenAI, Anthropic, Gemini, or Ollama on this computer. Keys stay in your system
              keychain.
            </p>
          </div>
        </div>
      </button>

      {#if selectedPath === "byok"}
        <div class="mt-4 space-y-4 border-t border-surface-500/30 pt-4">
          <div class="grid gap-2 sm:grid-cols-2">
            {#each BYOK_PROVIDERS as option (option.id)}
              <button
                type="button"
                class="settings-depth-card {byokProvider === option.id
                  ? 'settings-depth-card-active'
                  : ''}"
                disabled={wizard.busy}
                onclick={() => selectByokProvider(option.id)}
              >
                <span class="block text-sm font-medium text-surface-100">{option.label}</span>
                {#if option.id === "ollama"}
                  <span class="workshop-faint mt-1 block text-xs">
                    {ollamaReady ? "Ollama is running" : "Install ollama.com and start it"}
                  </span>
                {/if}
              </button>
            {/each}
          </div>

          {#if byokProvider !== "ollama"}
            <label class="block">
              <span class="block text-sm font-medium text-surface-100">API key</span>
              <input
                class="input mt-2 w-full font-mono text-sm"
                type="password"
                autocomplete="off"
                placeholder={BYOK_PROVIDERS.find((entry) => entry.id === byokProvider)?.keyHint}
                bind:value={apiKey}
                disabled={wizard.busy || validating}
              />
            </label>
          {/if}

          <label class="block">
            <span class="block text-sm font-medium text-surface-100">Model</span>
            {#if byokProvider === "ollama" && (probe?.ollamaModels.length ?? 0) > 0}
              <select class="select mt-2 w-full" bind:value={model} disabled={wizard.busy}>
                {#each probe?.ollamaModels ?? [] as name (name)}
                  <option value={name}>{name}</option>
                {/each}
              </select>
            {:else}
              <input
                class="input mt-2 w-full font-mono text-sm"
                bind:value={model}
                disabled={wizard.busy || validating}
              />
            {/if}
          </label>
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
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy || probing || localLoading}
      onclick={() => {
        void refreshProbe();
        if (selectedPath === "offline") void refreshLocalInference();
      }}
    >
      Try again
    </button>
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
