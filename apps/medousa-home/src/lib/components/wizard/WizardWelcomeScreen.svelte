<script lang="ts">
  import { onMount } from "svelte";
  import {
    Brain,
    ChevronRight,
    Cloud,
    LoaderCircle,
    WifiOff,
  } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import {
    probeProviders,
    validateProviderKey,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";

  type WizardPath = "managed" | "byok" | "offline";
  type ByokProvider = "openai" | "anthropic" | "google" | "ollama";

  const BYOK_PROVIDERS: { id: ByokProvider; label: string; keyHint: string }[] = [
    { id: "openai", label: "OpenAI", keyHint: "sk-…" },
    { id: "anthropic", label: "Anthropic", keyHint: "sk-ant-…" },
    { id: "google", label: "Google Gemini", keyHint: "AI…" },
    { id: "ollama", label: "Ollama (local)", keyHint: "" },
  ];

  let selectedPath = $state<WizardPath | null>(null);
  let byokProvider = $state<ByokProvider>("ollama");
  let apiKey = $state("");
  let model = $state("llama3.2");
  let probe = $state<ProvidersProbeResult | null>(null);
  let probing = $state(true);
  let validating = $state(false);
  let statusMessage = $state<string | null>(null);

  const networkOnline = $derived(probe?.networkOnline ?? true);
  const ollamaReady = $derived(probe?.ollamaDetected ?? false);

  onMount(() => {
    void refreshProbe();
  });

  async function refreshProbe() {
    probing = true;
    statusMessage = null;
    try {
      probe = await probeProviders();
      if (byokProvider === "ollama" || selectedPath === "offline") {
        model = probe.suggestedOllamaModel ?? "llama3.2";
      }
      if (!probe.networkOnline && selectedPath === "managed") {
        selectedPath = null;
      }
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      probing = false;
    }
  }

  function selectPath(path: WizardPath) {
    selectedPath = path;
    statusMessage = null;
    if (path === "offline" || (path === "byok" && byokProvider === "ollama")) {
      byokProvider = "ollama";
      model = probe?.suggestedOllamaModel ?? "llama3.2";
    }
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

  async function continueSetup() {
    if (!selectedPath || selectedPath === "managed") return;

    validating = true;
    statusMessage = null;
    wizard.error = null;

    try {
      const provider = selectedPath === "offline" ? "ollama" : byokProvider;
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
    if (wizard.busy || validating || probing) return false;
    if (!selectedPath || selectedPath === "managed") return false;
    if (selectedPath === "offline" || byokProvider === "ollama") {
      return ollamaReady && model.trim().length > 0;
    }
    return apiKey.trim().length > 0 && model.trim().length > 0;
  });
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 1 of 3</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Welcome to Medousa
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    I'm your second brain — always here, always yours, always private. First, let's decide how I
    should think.
  </p>

  {#if probing}
    <div class="mt-6 flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Checking your machine…
    </div>
  {/if}

  <div class="mt-6 space-y-3">
    <button
      type="button"
      class="wizard-path-card {selectedPath === 'managed' ? 'wizard-path-card-active' : ''} {!networkOnline
        ? 'opacity-50'
        : ''}"
      disabled={!networkOnline || wizard.busy}
      onclick={() => selectPath("managed")}
    >
      <div class="flex items-start gap-3">
        <Cloud class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
        <div class="min-w-0 text-left">
          <p class="font-semibold text-surface-50">Recommended — Managed AI</p>
          <p class="mt-1 text-sm text-surface-300">
            {#if networkOnline}
              Medousa Cloud provisioning lands in a future update — use your own key or Ollama
              today.
            {:else}
              <span class="inline-flex items-center gap-1 text-warning-200">
                <WifiOff class="h-3.5 w-3.5" aria-hidden="true" />
                Offline — pick Bring your own model or Offline below.
              </span>
            {/if}
          </p>
        </div>
      </div>
    </button>

    <div
      class="wizard-path-card {selectedPath === 'byok' ? 'wizard-path-card-active' : ''}"
      role="group"
      aria-label="Bring your own model"
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
            <p class="font-semibold text-surface-50">Bring your own model</p>
            <p class="mt-1 text-sm text-surface-300">
              OpenAI, Anthropic, Gemini, or Ollama on this Mac — keys stay in your keychain.
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
                    {ollamaReady ? "Detected on :11434" : "Not running — install ollama.com"}
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

    <button
      type="button"
      class="wizard-path-card {selectedPath === 'offline' ? 'wizard-path-card-active' : ''}"
      disabled={wizard.busy}
      onclick={() => selectPath("offline")}
    >
      <div class="flex items-start gap-3 text-left">
        <WifiOff class="mt-0.5 h-5 w-5 shrink-0 text-surface-300" aria-hidden="true" />
        <div class="min-w-0">
          <p class="font-semibold text-surface-50">Offline — local only</p>
          <p class="mt-1 text-sm text-surface-300">
            {#if ollamaReady}
              Ollama detected — private inference on this machine, no cloud required.
            {:else}
              Install Ollama from ollama.com, pull a model, then come back and tap Try again.
            {/if}
          </p>
          {#if selectedPath === "offline" && ollamaReady}
            <p class="mt-2 text-xs text-primary-200">Model: {model}</p>
          {/if}
        </div>
      </div>
    </button>
  </div>

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
      disabled={wizard.busy || probing}
      onclick={() => void refreshProbe()}
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
        Starting Medousa Core…
      {:else}
        Continue
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
