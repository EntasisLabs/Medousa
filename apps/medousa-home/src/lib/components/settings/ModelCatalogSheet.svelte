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
  import { listProviders } from "$lib/utils/providersApi";
  import { badgesForCapability } from "$lib/utils/modelCapabilityCatalog";
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
  import { CUSTOM_PROVIDER_CATALOG_ID } from "$lib/utils/customProvider";
  import {
    isCustomProviderReady,
    resolveProviderBaseUrl,
    resolveRuntimeProviderId,
  } from "$lib/utils/providerSettings";
  import { resolveModelsForProvider } from "$lib/utils/resolveProviderModels";

  type PickerStep = "provider" | "model";

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
  let models = $state<ModelCapabilityRecord[]>([]);
  let loading = $state(false);
  let needsProviderSetup = $state(false);
  let providerSearch = $state("");
  let modelSearch = $state("");
  let manualModelId = $state("");
  let sheetInitKey = $state<string | null>(null);
  let loadSeq = 0;

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

  function targetKey(value: ModelPickerTarget): string {
    return JSON.stringify(value);
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
    providerSearch = "";
    modelSearch = "";
    manualModelId = "";
    models = [];
    needsProviderSetup = false;
  });

  onMount(() => {
    if (catalogProp) return;
    void listProviders().then((listed) => {
      localCatalog = listed;
    });
  });

  async function loadProviderModels(entry: ProviderCatalogEntry) {
    if (!target) return;
    const seq = ++loadSeq;
    loading = true;
    needsProviderSetup = false;
    try {
      if (entry.id === CUSTOM_PROVIDER_CATALOG_ID && !(await isCustomProviderReady())) {
        needsProviderSetup = true;
        models = [];
        return;
      }
      const next = await resolveModelsForProvider(entry, {
        capability: pickerRequiresVision(target) ? "vision" : undefined,
      });
      if (seq !== loadSeq) return;
      models = next;
    } finally {
      if (seq === loadSeq) loading = false;
    }
  }

  async function openProvider(entry: ProviderCatalogEntry) {
    selectedProvider = entry;
    step = "model";
    modelSearch = "";
    const current = selectionForTarget(workshopDefaults.draft, target!);
    const runtimeId = await resolveRuntimeProviderId(entry.id);
    manualModelId =
      current?.provider?.trim().toLowerCase() === runtimeId.toLowerCase()
        ? current.model.trim()
        : entry.defaultModel.trim();
    void loadProviderModels(entry);
  }

  function goBackToProviders() {
    step = "provider";
    selectedProvider = null;
    modelSearch = "";
    manualModelId = "";
    models = [];
    needsProviderSetup = false;
    loadSeq += 1;
  }

  function handleDismiss() {
    if (step === "model") {
      goBackToProviders();
      return;
    }
    onClose();
  }

  async function buildSelection(modelId: string): Promise<InferenceTarget> {
    const entry = selectedProvider!;
    return {
      provider: await resolveRuntimeProviderId(entry.id),
      model: modelId.trim(),
      baseUrl: await resolveProviderBaseUrl(entry),
    };
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
    await onSelect(await buildSelection(record.modelId));
    onClose();
  }

  async function confirmManualModel() {
    const modelId = manualModelId.trim();
    if (!modelId || !selectedProvider) return;
    await onSelect(await buildSelection(modelId));
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
      class="model-catalog-sheet {step === 'provider' ? 'model-catalog-sheet-narrow' : ''}"
      role="dialog"
      aria-modal="true"
      aria-label={step === "provider" ? pickerTitle(target) : selectedProvider?.label ?? "Model"}
      transition:fly={{ y: 28, duration: 280, easing: cubicOut }}
    >
      <header class="model-catalog-sheet-header">
        {#if step === "model"}
          <button
            type="button"
            class="model-catalog-sheet-back"
            aria-label="Back to providers"
            onclick={goBackToProviders}
          >
            <ChevronLeft size={18} />
          </button>
        {/if}
        <div class="min-w-0 flex-1">
          <h3 class="model-catalog-sheet-title">
            {step === "provider" ? pickerTitle(target) : selectedProvider?.label ?? "Model"}
          </h3>
          <p class="model-catalog-sheet-subtitle">
            {step === "provider"
              ? "Choose a provider — configure keys and URLs under Providers."
              : "Pick from the catalog or enter a model ID."}
          </p>
        </div>
        <button type="button" class="model-catalog-sheet-close" aria-label="Close" onclick={onClose}>
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
                onclick={() => void openProvider(entry)}
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
      {:else if selectedProvider}
        {#if needsProviderSetup}
          <div class="model-catalog-custom-form">
            <p class="model-catalog-empty">
              Configure {selectedProvider.label} under the <strong>Providers</strong> tab first
              (provider id, API URL, and key if needed).
            </p>
          </div>
        {:else}
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
      {/if}
    </div>
  </div>
{/if}
