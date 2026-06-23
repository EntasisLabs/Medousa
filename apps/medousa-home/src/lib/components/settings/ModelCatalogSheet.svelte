<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { ChevronLeft, ChevronRight, Search, X } from "@lucide/svelte";
  import type { ProviderCatalogEntry, ProvidersListResult } from "$lib/types/providers";
  import {
    filterProviders,
    groupProvidersByCategory,
  } from "$lib/types/providers";
  import { findCatalogProvider, listProviderModels, listProviders } from "$lib/utils/providersApi";
  import {
    badgesForCapability,
    defaultProviderRecords,
    listModelCatalog,
    recordsFromModelIds,
  } from "$lib/utils/modelCapabilityCatalog";
  import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
  import ModelCapabilityBadges from "$lib/components/settings/ModelCapabilityBadges.svelte";
  import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import { providerMonogram } from "$lib/utils/chatModelPicker";
  import type { InferenceTarget } from "$lib/types/inferenceProfiles";
  import type { ModelPickerTarget } from "$lib/utils/modelAssignment";
  import {
    excludedProvidersForTarget,
    pickerAllowsClear,
    pickerRequiresVision,
    pickerTitle,
    providerIdsForTarget,
    selectionForTarget,
  } from "$lib/utils/modelAssignment";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import {
    CUSTOM_PROVIDER_CATALOG_ID,
    isConfiguredCustomProvider,
    isCustomCatalogEntry,
    isValidBaseUrl,
    normalizeBaseUrl,
    normalizeCustomProviderId,
  } from "$lib/utils/customProvider";
  import { messagingSaveSecret } from "$lib/messaging";

  type PickerStep = "provider" | "custom" | "model";

  interface Props {
    open: boolean;
    target: ModelPickerTarget | null;
    catalog?: ProvidersListResult | null;
    onClose: () => void;
    onSelect: (selection: InferenceTarget | null) => void | Promise<void>;
  }

  let {
    open,
    target,
    catalog: catalogProp = null,
    onClose,
    onSelect,
  }: Props = $props();

  let localCatalog = $state<ProvidersListResult | null>(null);
  const catalog = $derived(catalogProp ?? localCatalog);
  let step = $state<PickerStep>("provider");
  let selectedProvider = $state<ProviderCatalogEntry | null>(null);
  let customProviderId = $state("");
  let customBaseUrl = $state("");
  let customApiKey = $state("");
  let customConfigError = $state<string | null>(null);
  let models = $state<ModelCapabilityRecord[]>([]);
  let loading = $state(false);
  let providerSearch = $state("");
  let modelSearch = $state("");
  let manualModelId = $state("");
  let sheetInitKey = $state<string | null>(null);
  let loadSeq = 0;

  const isCustomFlow = $derived(selectedProvider?.id === CUSTOM_PROVIDER_CATALOG_ID);

  const activeProviderId = $derived(
    isCustomFlow ? customProviderId.trim() : (selectedProvider?.id ?? ""),
  );

  const activeBaseUrl = $derived(
    isCustomFlow ? customBaseUrl.trim() : (selectedProvider?.defaultBaseUrl?.trim() ?? ""),
  );

  const canContinueCustom = $derived(
    normalizeCustomProviderId(customProviderId).length > 0 &&
      isValidBaseUrl(customBaseUrl),
  );

  const selectedKey = $derived.by(() => {
    if (!target || target.type === "favorite-add") return null;
    const current = selectionForTarget(workshopDefaults.draft, target);
    if (!current) return null;
    return modelPickKey(current.provider, current.model);
  });

  const visibleProviders = $derived.by(() => {
    if (!catalog || !target) return [];
    const allowed = providerIdsForTarget(target);
    const excluded = new Set(
      excludedProvidersForTarget(target).map((id) => id.toLowerCase()),
    );
    return catalog.providers.filter((entry) => {
      if (excluded.has(entry.id.toLowerCase())) return false;
      if (allowed && !allowed.includes(entry.id)) return false;
      return true;
    });
  });

  const filteredProviders = $derived(
    filterProviders(visibleProviders, providerSearch),
  );

  const groupedProviders = $derived(
    catalog ? groupProvidersByCategory(filteredProviders, catalog.categories) : [],
  );

  const filteredModels = $derived.by(() => {
    const needle = modelSearch.trim().toLowerCase();
    if (!needle) return models;
    return models.filter(
      (record) =>
        record.modelId.toLowerCase().includes(needle) ||
        (record.displayName?.toLowerCase().includes(needle) ?? false),
    );
  });

  const canUseManualModel = $derived(manualModelId.trim().length > 0);

  const sheetTitle = $derived.by(() => {
    if (!target) return "Model";
    if (step === "provider") return pickerTitle(target);
    if (step === "custom") return "Custom provider";
    if (isCustomFlow && customProviderId.trim()) return customProviderId.trim();
    return selectedProvider?.label ?? "Model";
  });

  const sheetSubtitle = $derived.by(() => {
    if (step === "provider") return "Choose a provider.";
    if (step === "custom") return "Set the provider id, API URL, and optional key.";
    if (isCustomFlow && customBaseUrl.trim()) return customBaseUrl.trim();
    return "Pick from the catalog or enter a model ID.";
  });

  function targetKey(value: ModelPickerTarget): string {
    return JSON.stringify(value);
  }

  function resetCustomFields() {
    customProviderId = "";
    customBaseUrl = "";
    customApiKey = "";
    customConfigError = null;
  }

  function prefillCustomFromSelection(current: InferenceTarget) {
    customProviderId = current.provider.trim();
    customBaseUrl = current.baseUrl?.trim() ?? "";
    customApiKey = "";
    selectedProvider = catalog
      ? (findCatalogProvider(catalog, CUSTOM_PROVIDER_CATALOG_ID) ?? null)
      : null;
  }

  $effect(() => {
    if (!open) {
      sheetInitKey = null;
      return;
    }
    if (!target || !catalog) return;

    const initKey = targetKey(target);
    if (sheetInitKey === initKey) return;

    sheetInitKey = initKey;
    step = "provider";
    selectedProvider = null;
    resetCustomFields();
    providerSearch = "";
    modelSearch = "";
    manualModelId = "";
    models = [];

    const current = selectionForTarget(workshopDefaults.draft, target);
    if (current && isConfiguredCustomProvider(current, catalog)) {
      prefillCustomFromSelection(current);
      step = "custom";
    }
  });

  onMount(() => {
    if (catalogProp) return;
    void listProviders().then((listed) => {
      localCatalog = listed;
    });
  });

  function providerEntry(providerId: string): ProviderCatalogEntry | undefined {
    return catalog ? findCatalogProvider(catalog, providerId) : undefined;
  }

  async function resolveModelsForProvider(
    providerId: string,
    baseUrl?: string | null,
    apiKey?: string | null,
  ): Promise<ModelCapabilityRecord[]> {
    if (!target) return [];
    const normalizedProvider = providerId.trim().toLowerCase();

    if (!isCustomCatalogEntry(providerId)) {
      try {
        const response = await listModelCatalog({
          provider: providerId,
          capability: pickerRequiresVision(target) ? "vision" : undefined,
        });
        const fromCatalog = response.models.filter(
          (record) => record.provider.trim().toLowerCase() === normalizedProvider,
        );
        if (fromCatalog.length > 0) return fromCatalog;
      } catch {
        // Fall through to live provider listing / defaults.
      }
    }

    try {
      const entry = providerEntry(providerId);
      const live = await listProviderModels({
        provider: providerId,
        baseUrl: baseUrl?.trim() || entry?.defaultBaseUrl || undefined,
        apiKey: apiKey?.trim() || undefined,
      });
      if (live.models.length > 0) {
        return recordsFromModelIds(providerId, live.models, live.source);
      }
    } catch {
      // Fall through to provider default.
    }

    const entry = providerEntry(providerId);
    return entry ? defaultProviderRecords(entry) : [];
  }

  async function loadActiveProviderModels() {
    if (!target || !activeProviderId) return;
    const seq = ++loadSeq;
    loading = true;
    try {
      const next = await resolveModelsForProvider(
        activeProviderId,
        activeBaseUrl || null,
        customApiKey || null,
      );
      if (seq !== loadSeq) return;
      models = next;
    } finally {
      if (seq === loadSeq) loading = false;
    }
  }

  function openProvider(entry: ProviderCatalogEntry) {
    if (entry.id === CUSTOM_PROVIDER_CATALOG_ID) {
      selectedProvider = entry;
      step = "custom";
      customConfigError = null;
      const current = selectionForTarget(workshopDefaults.draft, target!);
      if (current && isConfiguredCustomProvider(current, catalog!)) {
        prefillCustomFromSelection(current);
      } else {
        customProviderId = "";
        customBaseUrl = "";
        customApiKey = "";
      }
      return;
    }

    selectedProvider = entry;
    step = "model";
    modelSearch = "";
    const current = selectionForTarget(workshopDefaults.draft, target!);
    manualModelId =
      current?.provider?.trim().toLowerCase() === entry.id.toLowerCase()
        ? current.model.trim()
        : entry.defaultModel.trim();
    void loadActiveProviderModels();
  }

  async function continueFromCustom() {
    customConfigError = null;
    const providerId = normalizeCustomProviderId(customProviderId);
    const baseUrl = normalizeBaseUrl(customBaseUrl);
    if (!providerId) {
      customConfigError = "Enter a provider id (e.g. openai, vllm).";
      return;
    }
    if (!isValidBaseUrl(baseUrl)) {
      customConfigError = "Enter a valid http(s) API base URL.";
      return;
    }

    customProviderId = providerId;
    customBaseUrl = baseUrl;

    const key = customApiKey.trim();
    if (key) {
      try {
        await messagingSaveSecret(`api_key_${providerId}`, key);
      } catch (err) {
        customConfigError =
          err instanceof Error ? err.message : "Could not store API key on this device.";
        return;
      }
    }

    step = "model";
    modelSearch = "";
    const current = selectionForTarget(workshopDefaults.draft, target!);
    manualModelId =
      current?.provider?.trim().toLowerCase() === providerId &&
      (current.baseUrl?.trim() ?? "") === baseUrl
        ? current.model.trim()
        : "";
    void loadActiveProviderModels();
  }

  function goBackOneStep() {
    if (step === "model") {
      if (isCustomFlow) {
        step = "custom";
      } else {
        step = "provider";
        selectedProvider = null;
      }
      modelSearch = "";
      manualModelId = "";
      models = [];
      loadSeq += 1;
      return;
    }
    if (step === "custom") {
      step = "provider";
      selectedProvider = null;
      resetCustomFields();
    }
  }

  function handleDismiss() {
    if (step === "model" || step === "custom") {
      goBackOneStep();
      return;
    }
    onClose();
  }

  function selectionPayload(modelId: string): InferenceTarget {
    const provider = activeProviderId;
    const baseUrl = activeBaseUrl || null;
    return { provider, model: modelId.trim(), baseUrl };
  }

  function displayName(record: ModelCapabilityRecord): string {
    return (
      record.displayName?.trim() ||
      resolveModelDisplayLabel(record.provider, record.modelId, 40)
    );
  }

  function showSlug(record: ModelCapabilityRecord): boolean {
    const name = displayName(record);
    return name.trim().toLowerCase() !== record.modelId.trim().toLowerCase();
  }

  function isSelected(record: ModelCapabilityRecord): boolean {
    const key = modelPickKey(record.provider, record.modelId);
    return selectedKey === key;
  }

  async function pickModel(record: ModelCapabilityRecord) {
    await onSelect(selectionPayload(record.modelId));
    onClose();
  }

  async function confirmManualModel() {
    const modelId = manualModelId.trim();
    if (!modelId || !selectedProvider) return;
    await onSelect(selectionPayload(modelId));
    onClose();
  }

  function handleBackdrop(event: MouseEvent) {
    if (event.target === event.currentTarget) handleDismiss();
  }
</script>

{#if open && target}
  <div
    class="model-catalog-backdrop"
    role="presentation"
    transition:fade={{ duration: 180 }}
    onclick={handleBackdrop}
    onkeydown={(event) => {
      if (event.key === "Escape") handleDismiss();
    }}
  >
    <div
      class="model-catalog-sheet {step !== 'model' ? 'model-catalog-sheet-narrow' : ''}"
      role="dialog"
      aria-modal="true"
      aria-label={sheetTitle}
      transition:fly={{ y: 28, duration: 280, easing: cubicOut }}
    >
      <header class="model-catalog-sheet-header">
        {#if step === "model" || step === "custom"}
          <button
            type="button"
            class="model-catalog-sheet-back"
            aria-label="Back"
            onclick={goBackOneStep}
          >
            <ChevronLeft size={18} />
          </button>
        {/if}
        <div class="min-w-0 flex-1">
          <h3 class="model-catalog-sheet-title">{sheetTitle}</h3>
          <p class="model-catalog-sheet-subtitle">{sheetSubtitle}</p>
        </div>
        <button
          type="button"
          class="model-catalog-sheet-close"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={18} />
        </button>
      </header>

      {#if step === "provider"}
        <label class="model-catalog-search">
          <Search size={15} class="model-catalog-search-icon" />
          <input
            type="search"
            class="model-catalog-search-input"
            placeholder="Search providers"
            bind:value={providerSearch}
          />
        </label>

        <div class="model-catalog-provider-list">
          {#if pickerAllowsClear(target)}
            <button
              type="button"
              class="model-catalog-provider-row model-catalog-provider-row-clear"
              onclick={async () => {
                await onSelect(null);
                onClose();
              }}
            >
              <span class="model-catalog-provider-row-copy">
                <span class="model-catalog-provider-row-label">None</span>
                <span class="model-catalog-provider-row-hint">Clear this backup slot</span>
              </span>
            </button>
          {/if}

          {#each groupedProviders as group (group.category.id)}
            <p class="model-catalog-provider-group-label">{group.category.label}</p>
            {#each group.providers as entry (entry.id)}
              <button
                type="button"
                class="model-catalog-provider-row"
                onclick={() => openProvider(entry)}
              >
                <span class="model-catalog-tile-monogram" aria-hidden="true">
                  {providerMonogram(entry.id)}
                </span>
                <span class="model-catalog-provider-row-copy">
                  <span class="model-catalog-provider-row-label">{entry.label}</span>
                  <span class="model-catalog-provider-row-hint">{entry.blurb}</span>
                </span>
                <ChevronRight size={16} class="model-catalog-provider-row-chevron" aria-hidden="true" />
              </button>
            {/each}
          {/each}

          {#if groupedProviders.length === 0}
            <p class="model-catalog-empty">No providers match — try another search.</p>
          {/if}
        </div>
      {:else if step === "custom"}
        <div class="model-catalog-custom-form">
          <label class="model-catalog-custom-field">
            <span class="model-catalog-custom-label">Provider id</span>
            <span class="model-catalog-custom-hint">Rust-genai adapter name (e.g. openai, vllm).</span>
            <input
              type="text"
              class="model-catalog-manual-input"
              placeholder="openai"
              bind:value={customProviderId}
              autocapitalize="off"
              autocomplete="off"
              spellcheck="false"
            />
          </label>

          <label class="model-catalog-custom-field">
            <span class="model-catalog-custom-label">API base URL</span>
            <span class="model-catalog-custom-hint">OpenAI-compatible root, usually ending in /v1.</span>
            <input
              type="url"
              class="model-catalog-manual-input"
              placeholder="https://your-host.example.com/v1"
              bind:value={customBaseUrl}
              autocapitalize="off"
              autocomplete="off"
              spellcheck="false"
            />
          </label>

          <label class="model-catalog-custom-field">
            <span class="model-catalog-custom-label">API key</span>
            <span class="model-catalog-custom-hint">Optional — stored on this device only.</span>
            <input
              type="password"
              class="model-catalog-manual-input"
              placeholder="sk-…"
              bind:value={customApiKey}
              autocomplete="off"
            />
          </label>

          {#if customConfigError}
            <p class="model-catalog-custom-error">{customConfigError}</p>
          {/if}

          <button
            type="button"
            class="model-catalog-manual-btn model-catalog-custom-continue"
            disabled={!canContinueCustom}
            onclick={() => void continueFromCustom()}
          >
            Continue to models
          </button>
        </div>
      {:else if selectedProvider}
        <label class="model-catalog-search">
          <Search size={15} class="model-catalog-search-icon" />
          <input
            type="search"
            class="model-catalog-search-input"
            placeholder="Search models"
            bind:value={modelSearch}
          />
        </label>

        <div class="model-catalog-grid">
          {#if loading}
            <p class="model-catalog-empty">Loading models…</p>
          {:else if filteredModels.length === 0}
            <p class="model-catalog-empty">
              No catalog models match — enter one manually below.
            </p>
          {:else}
            {#each filteredModels as record (`${record.provider}:${record.modelId}`)}
              {@const selected = isSelected(record)}
              <button
                type="button"
                class="model-catalog-tile {selected ? 'model-catalog-tile-selected' : ''}"
                onclick={() => void pickModel(record)}
              >
                <span class="model-catalog-tile-head">
                  <span class="model-catalog-tile-monogram" aria-hidden="true">
                    {providerMonogram(record.provider)}
                  </span>
                  <span class="model-catalog-tile-name">{displayName(record)}</span>
                </span>
                {#if showSlug(record)}
                  <span class="model-catalog-tile-meta">{record.modelId}</span>
                {/if}
                <ModelCapabilityBadges badges={badgesForCapability(record)} compact />
              </button>
            {/each}
          {/if}
        </div>

        <div class="model-catalog-manual">
          <label class="model-catalog-manual-label">
            <span class="model-catalog-manual-title">Enter model ID</span>
            <input
              type="text"
              class="model-catalog-manual-input"
              placeholder={selectedProvider.defaultModel || "e.g. gpt-4o-mini"}
              bind:value={manualModelId}
              onkeydown={(event) => {
                if (event.key === "Enter" && canUseManualModel) {
                  event.preventDefault();
                  void confirmManualModel();
                }
              }}
            />
          </label>
          <button
            type="button"
            class="model-catalog-manual-btn"
            disabled={!canUseManualModel}
            onclick={() => void confirmManualModel()}
          >
            Use this model
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
