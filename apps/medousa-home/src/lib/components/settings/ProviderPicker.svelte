<script lang="ts">
  import { onMount } from "svelte";
  import { Check, LoaderCircle, Search } from "@lucide/svelte";
  import type { ProviderCatalogEntry, ProvidersListResult } from "$lib/types/providers";
  import { filterProviders, groupProvidersByCategory } from "$lib/types/providers";
  import {
    findCatalogProvider,
    listProviderModels,
    listProviders,
    probeProviders,
    validateProviderKey,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";

  interface Props {
    providerId: string;
    model: string;
    apiKey?: string;
    baseUrl?: string;
    disabled?: boolean;
    compact?: boolean;
    excludeProviderIds?: string[];
    showValidate?: boolean;
    onProviderChange: (id: string, entry: ProviderCatalogEntry) => void;
    onModelChange: (model: string) => void;
    onApiKeyChange?: (key: string) => void;
    onBaseUrlChange?: (url: string) => void;
    onStatus?: (message: string | null, ok?: boolean) => void;
  }

  let {
    providerId,
    model,
    apiKey = "",
    baseUrl = "",
    disabled = false,
    compact = false,
    excludeProviderIds = [],
    showValidate = true,
    onProviderChange,
    onModelChange,
    onApiKeyChange,
    onBaseUrlChange,
    onStatus,
  }: Props = $props();

  let catalog = $state<ProvidersListResult | null>(null);
  let probe = $state<ProvidersProbeResult | null>(null);
  let search = $state("");
  let loading = $state(true);
  let validating = $state(false);
  let validatedOk = $state<boolean | null>(null);
  let liveModels = $state<string[]>([]);
  let loadingModels = $state(false);
  let modelsMessage = $state<string | null>(null);

  const selected = $derived(
    catalog ? findCatalogProvider(catalog, providerId) : undefined,
  );

  const filtered = $derived(
    catalog
      ? filterProviders(catalog.providers, search, excludeProviderIds)
      : [],
  );

  const grouped = $derived(
    catalog ? groupProvidersByCategory(filtered, catalog.categories) : [],
  );

  onMount(() => {
    void bootstrap();
  });

  async function bootstrap() {
    loading = true;
    try {
      const [listed, probed] = await Promise.all([listProviders(), probeProviders()]);
      catalog = listed;
      probe = probed;
      if (!selected && listed.providers.length > 0) {
        const fallback =
          findCatalogProvider(listed, providerId) ??
          listed.providers.find((entry) => entry.id === "openai") ??
          listed.providers[0];
        if (fallback) {
          onProviderChange(fallback.id, fallback);
          if (!model.trim()) onModelChange(fallback.defaultModel);
        }
      }
    } catch (err) {
      onStatus?.(err instanceof Error ? err.message : String(err), false);
    } finally {
      loading = false;
    }
  }

  function selectProvider(entry: ProviderCatalogEntry) {
    validatedOk = null;
    liveModels = [];
    modelsMessage = null;
    onProviderChange(entry.id, entry);
    onModelChange(
      entry.id === "ollama"
        ? (probe?.suggestedOllamaModel ?? entry.defaultModel)
        : entry.defaultModel,
    );
    if (entry.supportsCustomBaseUrl && entry.defaultBaseUrl) {
      onBaseUrlChange?.(entry.defaultBaseUrl);
    } else if (!entry.supportsCustomBaseUrl) {
      onBaseUrlChange?.("");
    }
    onStatus?.(null);
  }

  async function runBrowseModels() {
    if (!selected) return;
    loadingModels = true;
    modelsMessage = null;
    try {
      const result = await listProviderModels({
        provider: selected.id,
        apiKey: apiKey.trim() || undefined,
        baseUrl: baseUrl.trim() || selected.defaultBaseUrl || undefined,
      });
      liveModels = result.models;
      if (result.models.length === 0) {
        modelsMessage = "No models returned — enter a model ID manually.";
      } else {
        modelsMessage = `${result.models.length} models from ${result.source}`;
        if (!model.trim() || !result.models.includes(model.trim())) {
          onModelChange(result.models[0] ?? model);
        }
      }
    } catch (err) {
      liveModels = [];
      modelsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      loadingModels = false;
    }
  }

  async function runValidate() {
    if (!selected) return;
    validating = true;
    validatedOk = null;
    onStatus?.(null);
    try {
      const result = await validateProviderKey({
        provider: selected.id,
        apiKey,
        baseUrl: baseUrl.trim() || selected.defaultBaseUrl,
      });
      validatedOk = result.ok;
      onStatus?.(result.message, result.ok);
      if (result.suggestedModel?.trim()) {
        onModelChange(result.suggestedModel);
      }
      if (result.ok) {
        await runBrowseModels();
      }
    } catch (err) {
      validatedOk = false;
      onStatus?.(err instanceof Error ? err.message : String(err), false);
    } finally {
      validating = false;
    }
  }
</script>

<div class="provider-picker space-y-4">
  {#if loading}
    <p class="workshop-faint flex items-center gap-2 text-sm">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading providers…
    </p>
  {:else}
    <label class="block">
      <span class="block text-sm font-medium text-surface-100">Find a provider</span>
      <div class="relative mt-2">
        <Search
          class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-surface-500"
          aria-hidden="true"
        />
        <input
          class="input w-full pl-9"
          type="search"
          placeholder="Search OpenAI, DeepSeek, Groq…"
          bind:value={search}
          disabled={disabled}
        />
      </div>
    </label>

    <div class="max-h-56 space-y-3 overflow-y-auto pr-1">
      {#each grouped as group (group.category.id)}
        <div>
          <p class="workshop-label mb-1.5">{group.category.label}</p>
          <div class="grid gap-2 {compact ? '' : 'sm:grid-cols-2'}">
            {#each group.providers as entry (entry.id)}
              <button
                type="button"
                class="settings-depth-card text-left {providerId === entry.id
                  ? 'settings-depth-card-active'
                  : ''}"
                disabled={disabled}
                onclick={() => selectProvider(entry)}
              >
                <span class="block text-sm font-medium text-surface-100">{entry.label}</span>
                <span class="workshop-faint mt-1 block text-xs leading-snug">{entry.blurb}</span>
                {#if entry.id === "ollama" && probe}
                  <span class="workshop-faint mt-1 block text-xs">
                    {probe.ollamaDetected ? "Ollama is running" : "Install ollama.com and start it"}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/each}
      {#if grouped.length === 0}
        <p class="workshop-faint text-sm">No providers match “{search}”.</p>
      {/if}
    </div>

    {#if selected}
      {#if selected.needsApiKey && onApiKeyChange}
        <label class="block">
          <span class="block text-sm font-medium text-surface-100">API key</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Stored securely on this device — never sent to Medousa cloud
          </span>
          <input
            class="input mt-2 w-full font-mono text-sm"
            type="password"
            autocomplete="off"
            placeholder={selected.keyHint ?? "Paste API key"}
            value={apiKey}
            disabled={disabled || validating}
            oninput={(event) => {
              validatedOk = null;
              onApiKeyChange((event.currentTarget as HTMLInputElement).value);
            }}
          />
        </label>
      {/if}

      {#if selected.supportsCustomBaseUrl && onBaseUrlChange}
        <label class="block">
          <span class="block text-sm font-medium text-surface-100">API base URL</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Optional — only change for self-hosted or enterprise endpoints
          </span>
          <input
            class="input mt-2 w-full font-mono text-sm"
            placeholder={selected.defaultBaseUrl ?? "https://…"}
            value={baseUrl}
            disabled={disabled || validating}
            oninput={(event) => {
              validatedOk = null;
              onBaseUrlChange((event.currentTarget as HTMLInputElement).value);
            }}
          />
        </label>
      {/if}

      <label class="block">
        <span class="block text-sm font-medium text-surface-100">Model</span>
        {#if selected.id === "ollama" && (probe?.ollamaModels.length ?? 0) > 0}
          <select
            class="select mt-2 w-full font-mono text-sm"
            value={model}
            disabled={disabled || validating}
            onchange={(event) =>
              onModelChange((event.currentTarget as HTMLSelectElement).value)}
          >
            {#each probe?.ollamaModels ?? [] as name (name)}
              <option value={name}>{name}</option>
            {/each}
          </select>
        {:else if liveModels.length > 0}
          <select
            class="select mt-2 w-full font-mono text-sm"
            value={model}
            disabled={disabled || validating || loadingModels}
            onchange={(event) =>
              onModelChange((event.currentTarget as HTMLSelectElement).value)}
          >
            {#each liveModels as name (name)}
              <option value={name}>{name}</option>
            {/each}
          </select>
          {#if modelsMessage}
            <p class="workshop-faint mt-1.5 text-xs">{modelsMessage}</p>
          {/if}
        {:else}
          <input
            class="input mt-2 w-full font-mono text-sm"
            value={model}
            disabled={disabled || validating}
            oninput={(event) =>
              onModelChange((event.currentTarget as HTMLInputElement).value)}
          />
          {#if modelsMessage}
            <p class="workshop-faint mt-1.5 text-xs text-warning-400">{modelsMessage}</p>
          {/if}
        {/if}
      </label>

      {#if selected.id !== "ollama" && selected.id !== "medousa-local"}
        <button
          type="button"
          class="btn variant-ghost-surface min-h-9 text-sm"
          disabled={
            disabled ||
            validating ||
            loadingModels ||
            (selected.needsApiKey && !apiKey.trim())
          }
          onclick={() => void runBrowseModels()}
        >
          {#if loadingModels}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
            Loading models…
          {:else}
            Browse live models
          {/if}
        </button>
      {/if}

      {#if showValidate && (selected.needsApiKey || selected.id === "ollama")}
        <button
          type="button"
          class="btn variant-soft-primary min-h-9 text-sm"
          disabled={disabled || validating || (selected.needsApiKey && !apiKey.trim() && selected.id !== "ollama")}
          onclick={() => void runValidate()}
        >
          {#if validating}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
            Checking…
          {:else if validatedOk === true}
            <Check class="mr-2 inline h-4 w-4 text-success-400" aria-hidden="true" />
            Verified
          {:else}
            Test connection
          {/if}
        </button>
      {/if}
    {/if}
  {/if}
</div>
