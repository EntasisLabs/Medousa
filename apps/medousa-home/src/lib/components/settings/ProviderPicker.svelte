<script lang="ts">
  import { onMount } from "svelte";
  import { Check, LoaderCircle, Search } from "@lucide/svelte";
  import type { ProviderCatalogEntry, ProvidersListResult } from "$lib/types/providers";
  import { filterProviders, groupProvidersByCategory } from "$lib/types/providers";
  import {
    findCatalogProvider,
    listProviders,
    probeProviders,
    validateProviderKey,
    type ProvidersProbeResult,
  } from "$lib/utils/providersApi";
  import {
    badgesForModel,
    capabilityMapFromCatalog,
  } from "$lib/utils/modelCapabilityCatalog";
  import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
  import ModelCapabilityBadges from "$lib/components/settings/ModelCapabilityBadges.svelte";
  import {
    pickModelFromRecords,
    resolveModelsForProvider,
  } from "$lib/utils/resolveProviderModels";

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
  let modelSearch = $state("");
  let loading = $state(true);
  let validating = $state(false);
  let validatedOk = $state<boolean | null>(null);
  let modelRecords = $state<ModelCapabilityRecord[]>([]);
  let loadingModels = $state(false);
  let modelsMessage = $state<string | null>(null);
  let capabilityMap = $state<Map<string, ModelCapabilityRecord>>(new Map());
  let loadSeq = 0;
  let showManualModel = $state(false);

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

  const filteredModels = $derived.by(() => {
    const needle = modelSearch.trim().toLowerCase();
    if (!needle) return modelRecords;
    return modelRecords.filter(
      (record) =>
        record.modelId.toLowerCase().includes(needle) ||
        (record.displayName?.toLowerCase().includes(needle) ?? false),
    );
  });

  const selectedModelBadges = $derived(
    selected ? badgesForModel(capabilityMap, selected.id, model) : [],
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
          await loadModelsForEntry(fallback, { preferSuggested: true });
        }
      } else if (selected) {
        await loadModelsForEntry(selected, { preferSuggested: true, keepCurrent: true });
      }
    } catch (err) {
      onStatus?.(err instanceof Error ? err.message : String(err), false);
    } finally {
      loading = false;
    }
  }

  async function loadModelsForEntry(
    entry: ProviderCatalogEntry,
    options?: { preferSuggested?: boolean; keepCurrent?: boolean },
  ) {
    const seq = ++loadSeq;
    loadingModels = true;
    modelsMessage = null;
    showManualModel = false;
    try {
      const next = await resolveModelsForProvider(entry, {
        apiKey: apiKey.trim() || undefined,
        baseUrl: baseUrl.trim() || entry.defaultBaseUrl || undefined,
      });
      if (seq !== loadSeq) return;
      modelRecords = next;
      capabilityMap = capabilityMapFromCatalog(next);

      const suggested =
        entry.id === "ollama"
          ? (probe?.suggestedOllamaModel ?? probe?.ollamaModels?.[0] ?? null)
          : null;
      const picked = pickModelFromRecords(next, {
        preferred: options?.preferSuggested ? suggested : null,
        current: options?.keepCurrent ? model : null,
        fallbackDefault: entry.defaultModel,
      });
      if (picked && picked !== model.trim()) {
        onModelChange(picked);
      } else if (!model.trim() && picked) {
        onModelChange(picked);
      }

      if (next.length === 0) {
        modelsMessage = "No models found — enter a model ID manually.";
        showManualModel = true;
      } else if (next.length === 1 && next[0]?.source === "catalog.default") {
        modelsMessage = "Using provider default (connect or browse for the full list).";
      } else {
        modelsMessage = `${next.length} model${next.length === 1 ? "" : "s"} available`;
      }
    } catch (err) {
      if (seq !== loadSeq) return;
      modelRecords = [];
      modelsMessage = err instanceof Error ? err.message : String(err);
      showManualModel = true;
      if (!model.trim()) onModelChange(entry.defaultModel);
    } finally {
      if (seq === loadSeq) loadingModels = false;
    }
  }

  function selectProvider(entry: ProviderCatalogEntry) {
    validatedOk = null;
    modelSearch = "";
    modelsMessage = null;
    onProviderChange(entry.id, entry);
    if (entry.supportsCustomBaseUrl && entry.defaultBaseUrl) {
      onBaseUrlChange?.(entry.defaultBaseUrl);
    } else if (!entry.supportsCustomBaseUrl) {
      onBaseUrlChange?.("");
    }
    onStatus?.(null);
    void loadModelsForEntry(entry, { preferSuggested: true });
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
        await loadModelsForEntry(selected, { preferSuggested: true, keepCurrent: true });
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

      <div class="block">
        <div class="flex items-center justify-between gap-2">
          <span class="block text-sm font-medium text-surface-100">Model</span>
          {#if loadingModels}
            <span class="workshop-faint flex items-center gap-1 text-xs">
              <LoaderCircle class="h-3 w-3 animate-spin" aria-hidden="true" />
              Loading…
            </span>
          {/if}
        </div>

        {#if modelRecords.length > 3}
          <div class="relative mt-2">
            <Search
              class="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-surface-500"
              aria-hidden="true"
            />
            <input
              class="input w-full pl-9 text-sm"
              type="search"
              placeholder="Search models…"
              bind:value={modelSearch}
              disabled={disabled || validating || loadingModels}
            />
          </div>
        {/if}

        {#if modelRecords.length > 0}
          <div class="provider-model-list mt-2 max-h-48 space-y-1 overflow-y-auto pr-1">
            {#each filteredModels as record (record.modelId)}
              {@const badges = badgesForModel(capabilityMap, selected.id, record.modelId)}
              <button
                type="button"
                class="provider-model-list-item {model === record.modelId
                  ? 'provider-model-list-item-active'
                  : ''}"
                disabled={disabled || validating || loadingModels}
                onclick={() => onModelChange(record.modelId)}
              >
                <span class="provider-model-list-name">
                  {record.displayName || record.modelId}
                </span>
                <ModelCapabilityBadges badges={badges} compact />
              </button>
            {/each}
            {#if filteredModels.length === 0}
              <p class="workshop-faint px-1 py-2 text-xs">No models match “{modelSearch}”.</p>
            {/if}
          </div>
        {/if}

        {#if showManualModel || modelRecords.length === 0}
          <input
            class="input mt-2 w-full font-mono text-sm"
            value={model}
            placeholder={selected.defaultModel || "model id"}
            disabled={disabled || validating}
            oninput={(event) =>
              onModelChange((event.currentTarget as HTMLInputElement).value)}
          />
        {:else if selectedModelBadges.length > 0 && modelRecords.length <= 3}
          <div class="mt-2">
            <ModelCapabilityBadges badges={selectedModelBadges} />
          </div>
        {/if}

        {#if modelsMessage}
          <p class="workshop-faint mt-1.5 text-xs">{modelsMessage}</p>
        {/if}

        {#if modelRecords.length > 0 && !showManualModel}
          <button
            type="button"
            class="mt-1.5 text-xs text-primary-300 hover:text-primary-200"
            disabled={disabled}
            onclick={() => (showManualModel = true)}
          >
            Enter model ID manually
          </button>
        {/if}
      </div>

      {#if selected.needsApiKey && onApiKeyChange}
        <button
          type="button"
          class="btn variant-ghost-surface min-h-9 text-sm"
          disabled={
            disabled ||
            validating ||
            loadingModels ||
            !apiKey.trim()
          }
          onclick={() => void loadModelsForEntry(selected, { keepCurrent: true })}
        >
          {#if loadingModels}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
            Loading models…
          {:else}
            Refresh models
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

<style>
  :global(.provider-model-list-item) {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
    padding: 0.45rem 0.65rem;
    text-align: left;
    transition: border-color 120ms ease, background 120ms ease;
  }

  :global(.provider-model-list-item:hover:not(:disabled)) {
    border-color: color-mix(in srgb, var(--color-primary-400) 45%, transparent);
  }

  :global(.provider-model-list-item-active) {
    border-color: color-mix(in srgb, var(--color-primary-400) 70%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  :global(.provider-model-list-name) {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }
</style>
