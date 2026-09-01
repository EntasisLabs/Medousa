<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { cubicIn, cubicOut } from "svelte/easing";
  import {
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Search,
    X,
  } from "@lucide/svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriIos, isTauriMobilePlatform } from "$lib/platform";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import { providerMonogram, resolveProviderLabel } from "$lib/utils/chatModelPicker";
  import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
  import {
    filterProviders,
    groupProvidersByCategory,
    type ProviderCatalogEntry,
  } from "$lib/types/providers";
  import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
  import { listProviders } from "$lib/utils/providersApi";
  import { recordsFromModelIds } from "$lib/utils/modelCapabilityCatalog";
  import { CUSTOM_PROVIDER_CATALOG_ID } from "$lib/utils/customProvider";
  import { isCustomProviderReady, resolveRuntimeProviderId } from "$lib/utils/providerSettings";
  import { resolveModelsForProvider } from "$lib/utils/resolveProviderModels";
  import {
    chatGptOAuthReady,
    getChatGptOAuthConnection,
    listChatGptOAuthModels,
  } from "$lib/utils/chatgptOAuth";
  import { mobileComposerRoutingHint } from "$lib/platformCopy";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import { haptic } from "$lib/haptics";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import type { DepthMode, ReasoningEffortMode } from "$lib/types/runtime";
  import { REASONING_EFFORT_OPTIONS, reasoningEffortLabel } from "$lib/types/reasoningEffort";
  import { fetchLocalModels } from "$lib/utils/localInferenceApi";

  type SheetView = "main" | "provider" | "model" | "voice" | "stance" | "reasoning";

  interface Props {
    disabled?: boolean;
    /** Quiet footer trigger — model name only. */
    quiet?: boolean;
  }

  let { disabled = false, quiet = false }: Props = $props();

  let open = $state(false);
  let sheetView = $state<SheetView>("main");
  let displayView = $state<SheetView>("main");
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let panelVisible = $state(true);
  let navigating = $state(false);
  let loadingCatalog = $state(true);
  let catalogSnapshot = $state<Awaited<ReturnType<typeof listProviders>> | null>(null);
  let selectedProvider = $state<ProviderCatalogEntry | null>(null);
  let providerSearch = $state("");
  let modelSearch = $state("");
  let manualModelId = $state("");
  let models = $state<ModelCapabilityRecord[]>([]);
  let modelsLoading = $state(false);
  let providerSetupSection = $state<"agent" | "connections" | null>(null);
  let providerSetupMessage = $state<string | null>(null);
  let modelLoadError = $state<string | null>(null);
  let modelActionError = $state<string | null>(null);
  let localModelRequiresSetup = $state(false);
  let installedLocalModelIds = $state<Set<string>>(new Set());
  let localInstallStateLoaded = $state(false);
  let modelLoadSeq = 0;

  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const modelLabel = $derived(resolveModelDisplayLabel(runtime.provider, runtime.model));
  const voiceLabel = $derived(voicePresets.activePreset.name);
  const depthLabel = $derived(
    DEPTH_CHARTER_OPTIONS.find((option) => option.id === runtime.depthMode)?.label ?? "Standard",
  );
  const reasoningLabel = $derived(reasoningEffortLabel(runtime.reasoningEffort));
  const pickerDisabled = $derived(disabled || runtime.savingControls || voicePresets.saving);
  const favoriteModels = $derived(workshopDefaults.favoriteModels());
  const activeCatalogProviderId = $derived.by(() => {
    if (!catalogSnapshot) return runtime.provider;
    const activeProvider = runtime.provider.trim().toLowerCase();
    const direct = catalogSnapshot.providers.find(
      (entry) => entry.id.trim().toLowerCase() === activeProvider,
    );
    if (direct) return direct.id;
    return catalogSnapshot.providers.some((entry) => entry.id === CUSTOM_PROVIDER_CATALOG_ID)
      ? CUSTOM_PROVIDER_CATALOG_ID
      : runtime.provider;
  });
  const filteredProviders = $derived(
    catalogSnapshot ? filterProviders(catalogSnapshot.providers, providerSearch) : [],
  );
  const currentProviderEntry = $derived(
    filteredProviders.find((entry) => entry.id === activeCatalogProviderId) ?? null,
  );
  const groupedProviders = $derived(
    catalogSnapshot
      ? groupProvidersByCategory(
          filteredProviders.filter((entry) => entry.id !== currentProviderEntry?.id),
          catalogSnapshot.categories,
        )
      : [],
  );
  const filteredModels = $derived.by(() => {
    const needle = modelSearch.trim().toLowerCase();
    const visible = needle
      ? models.filter(
          (record) =>
            record.modelId.toLowerCase().includes(needle) ||
            (record.displayName?.toLowerCase().includes(needle) ?? false),
        )
      : models;
    return [...visible].sort((left, right) => {
      const leftSelected = modelPickKey(left.provider, left.modelId) === activeKey;
      const rightSelected = modelPickKey(right.provider, right.modelId) === activeKey;
      if (leftSelected === rightSelected) return 0;
      return leftSelected ? -1 : 1;
    });
  });
  const canUseManualModel = $derived(manualModelId.trim().length > 0 && !runtime.savingControls);
  const selectedLocalIosProvider = $derived(
    isTauriIos() && selectedProvider?.id.trim().toLowerCase() === "medousa-local",
  );
  const sheetTitle = $derived(
    sheetView === "main"
      ? "Turn settings"
      : sheetView === "provider"
        ? "Select model"
        : sheetView === "model"
          ? selectedProvider?.label ?? "Models"
          : sheetView === "voice"
            ? "Voice"
            : sheetView === "stance"
              ? "Stance"
              : "Reasoning",
  );
  const titleTransition = {
    in: { duration: 150, easing: cubicOut },
    out: { duration: 100, easing: cubicIn },
  };
  const subPanelOut = { duration: 120, easing: cubicIn };
  const subPanelIn = { duration: 150, easing: cubicOut };
  const SUB_PANEL_CLEAR_MS = 130;

  onMount(() => {
    void bootstrap();
    void voicePresets.load();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) closeSheet();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  async function bootstrap() {
    loadingCatalog = true;
    try {
      if (isTauriMobilePlatform() && !workshopDefaults.loaded) {
        await workshopDefaults.load().catch(() => {});
      }
      catalogSnapshot = await listProviders();
    } catch {
      catalogSnapshot = null;
    } finally {
      loadingCatalog = false;
    }
  }

  function resetModelDrillIn() {
    selectedProvider = null;
    modelSearch = "";
    manualModelId = "";
    models = [];
    modelsLoading = false;
    providerSetupSection = null;
    providerSetupMessage = null;
    modelLoadError = null;
    modelActionError = null;
    localModelRequiresSetup = false;
    installedLocalModelIds = new Set();
    localInstallStateLoaded = false;
    modelLoadSeq += 1;
  }

  function openSheet() {
    if (pickerDisabled) return;
    displayView = "main";
    sheetView = "main";
    panelVisible = true;
    navigating = false;
    providerSearch = "";
    resetModelDrillIn();
    open = true;
    if (!catalogSnapshot && !loadingCatalog) void bootstrap();
  }

  function closeSheet() {
    open = false;
    displayView = "main";
    sheetView = "main";
    panelVisible = true;
    navigating = false;
    providerSearch = "";
    resetModelDrillIn();
  }

  async function transitionToView(next: SheetView) {
    if (navigating || next === displayView) return;
    navigating = true;
    panelVisible = false;
    await new Promise((resolve) => setTimeout(resolve, SUB_PANEL_CLEAR_MS));
    displayView = next;
    sheetView = next;
    panelVisible = true;
    navigating = false;
  }

  function drillTo(view: Exclude<SheetView, "main" | "model">) {
    if (view === "provider") providerSearch = "";
    void transitionToView(view);
  }

  function goBack() {
    if (sheetView === "model") {
      modelLoadSeq += 1;
      modelsLoading = false;
      void transitionToView("provider").then(resetModelDrillIn);
      return;
    }
    void transitionToView("main");
  }

  function handleSheetSwipeBack(): boolean {
    if (sheetView === "main") return false;
    goBack();
    return true;
  }

  function dismissSheet() {
    haptic("light");
    closeSheet();
  }

  $effect(() => {
    if (!open || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: dismissSheet,
      onSwipeBack: handleSheetSwipeBack,
    });
  });

  async function openProvider(entry: ProviderCatalogEntry) {
    selectedProvider = entry;
    modelSearch = "";
    manualModelId = "";
    models = [];
    providerSetupSection = null;
    providerSetupMessage = null;
    modelLoadError = null;
    modelActionError = null;
    await transitionToView("model");
    void loadProviderModels(entry);
  }

  async function loadProviderModels(entry: ProviderCatalogEntry) {
    const seq = ++modelLoadSeq;
    modelsLoading = true;
    providerSetupSection = null;
    providerSetupMessage = null;
    modelLoadError = null;
    try {
      if (entry.id === CUSTOM_PROVIDER_CATALOG_ID && !(await isCustomProviderReady())) {
        if (seq !== modelLoadSeq) return;
        providerSetupSection = "agent";
        providerSetupMessage =
          "Configure the provider ID, API URL, and key before choosing one of its models.";
        return;
      }

      if (entry.id === "openai-codex") {
        const connection = await getChatGptOAuthConnection();
        if (seq !== modelLoadSeq) return;
        if (!chatGptOAuthReady(connection)) {
          providerSetupSection = "connections";
          providerSetupMessage =
            "Connect your ChatGPT account before choosing a subscription model.";
          return;
        }
        try {
          const live = await listChatGptOAuthModels();
          if (seq !== modelLoadSeq) return;
          if (live.models.length > 0) {
            models = recordsFromModelIds(entry.id, live.models, "chatgpt-account");
            return;
          }
        } catch {
          // Fall through to the shared catalog resolver.
        }
      }

      const next = await resolveModelsForProvider(entry);
      if (seq !== modelLoadSeq) return;
      models = next;
      if (isTauriIos() && entry.id.trim().toLowerCase() === "medousa-local") {
        try {
          const localModels = await fetchLocalModels();
          if (seq !== modelLoadSeq) return;
          installedLocalModelIds = new Set(
            localModels.installed
              .filter((model) => model.verified)
              .map((model) => model.modelId.trim()),
          );
          localInstallStateLoaded = true;
        } catch {
          // Keep the catalog visible, but do not pretend an unverified local
          // checkpoint is ready for chat.
          localInstallStateLoaded = false;
        }
      }
    } catch (error) {
      if (seq !== modelLoadSeq) return;
      models = [];
      modelLoadError = error instanceof Error ? error.message : "Models could not be loaded.";
    } finally {
      if (seq === modelLoadSeq) modelsLoading = false;
    }
  }

  async function applyModel(provider: string, model: string) {
    const nextProvider = provider.trim();
    const nextModel = model.trim();
    if (!nextProvider || !nextModel || runtime.savingControls) return;
    modelActionError = null;
    localModelRequiresSetup = false;
    const nextKey = modelPickKey(nextProvider, nextModel);
    if (nextKey !== activeKey) await runtime.applyModel(nextProvider, nextModel);
    if (modelPickKey(runtime.provider, runtime.model) !== nextKey) {
      modelActionError = runtime.controlsMessage ?? "That model could not be selected.";
      return;
    }
    haptic("light");
    await transitionToView("main");
    resetModelDrillIn();
  }

  async function selectModel(record: ModelCapabilityRecord) {
    if (!selectedProvider) return;
    if (selectedLocalIosProvider) {
      if (!localModelIsReady(record.modelId)) {
        showLocalModelSetup(record.modelId);
        return;
      }
    }
    await applyModel(await resolveRuntimeProviderId(selectedProvider.id), record.modelId);
  }

  async function confirmManualModel() {
    if (!selectedProvider || !canUseManualModel) return;
    if (selectedLocalIosProvider) {
      if (!localModelIsReady(manualModelId)) {
        showLocalModelSetup(manualModelId);
        return;
      }
    }
    await applyModel(await resolveRuntimeProviderId(selectedProvider.id), manualModelId);
  }

  function showLocalModelSetup(modelId: string) {
    const normalized = modelId.trim();
    localModelRequiresSetup = true;
    modelActionError = localInstallStateLoaded
      ? `${resolveModelDisplayLabel("medousa-local", normalized)} is not fully downloaded. Manage it under Private brain.`
      : "Medousa could not verify the models on this device. Open Private brain to refresh them.";
  }

  function localModelIsReady(modelId: string): boolean {
    return localInstallStateLoaded && installedLocalModelIds.has(modelId.trim());
  }

  function localModelNeedsSetup(modelId: string): boolean {
    return selectedLocalIosProvider && !localModelIsReady(modelId);
  }

  function displayModelName(record: ModelCapabilityRecord): string {
    return record.displayName?.trim() || resolveModelDisplayLabel(record.provider, record.modelId, 40);
  }

  function showModelSlug(record: ModelCapabilityRecord): boolean {
    return displayModelName(record).trim().toLowerCase() !== record.modelId.toLowerCase();
  }

  function openProviderSettings() {
    const section = providerSetupSection;
    closeSheet();
    if (!section) return;
    settingsNav.setActiveSection(section);
    layout.openMore("settings");
  }

  function openLocalBrainSettings() {
    closeSheet();
    settingsNav.setActiveSection("basement");
    layout.openMore("settings");
  }

  async function selectVoice(voiceId: string) {
    if (voiceId === voicePresets.activeVoiceId || voicePresets.saving) return;
    await voicePresets.setActiveVoiceId(voiceId);
    if (workshopDefaults.loaded) {
      workshopDefaults.draft = { ...workshopDefaults.draft, activeVoiceId: voiceId };
    }
  }

  async function selectDepth(mode: DepthMode) {
    if (mode === runtime.depthMode || runtime.savingControls) return;
    await runtime.setDepthMode(mode);
  }

  async function selectReasoning(mode: ReasoningEffortMode) {
    if (mode === runtime.reasoningEffort || runtime.savingControls) return;
    await runtime.setReasoningEffort(mode);
  }

  function handleSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSheet();
    }
  }
</script>

<div class="mobile-composer-turn" class:mobile-composer-turn-quiet={quiet}>
  <button
    type="button"
    class="mobile-composer-turn-trigger {quiet ? 'mobile-composer-turn-trigger--quiet' : ''} {open ? 'mobile-composer-turn-trigger-open' : ''}"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label="Model and turn settings: {modelLabel}, {depthLabel} stance, {voiceLabel} voice"
    disabled={pickerDisabled}
    onclick={openSheet}
  >
    <span class="mobile-composer-turn-trigger-label">
      {loadingCatalog ? "Model" : modelLabel}
      {#if !quiet}
        <span class="mobile-composer-turn-trigger-sep" aria-hidden="true">·</span>
        {depthLabel}
      {/if}
    </span>
    <ChevronDown size={13} class="mobile-composer-turn-trigger-chevron" />
  </button>
</div>

{#if open}
  <div
    class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeSheet();
    }}
  >
    <div
      bind:this={sheetEl}
      class="mobile-sheet mobile-turn-sheet"
      role="dialog"
      aria-modal="true"
      aria-label={sheetTitle}
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleSheetKeydown}
    >
      <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
      <header bind:this={headerEl} class="mobile-turn-sheet-header">
        {#if sheetView === "main"}
          <button type="button" class="mobile-turn-sheet-icon-btn" aria-label="Close" onclick={closeSheet}>
            <X size={18} strokeWidth={2} />
          </button>
        {:else}
          <button
            type="button"
            class="mobile-turn-sheet-icon-btn"
            aria-label={sheetView === "model" ? "Back to providers" : "Back to turn settings"}
            disabled={navigating}
            onclick={goBack}
          >
            <ChevronLeft size={20} strokeWidth={2} />
          </button>
        {/if}
        <h2 class="mobile-turn-sheet-title">
          {#key sheetView}
            <span
              class="mobile-turn-sheet-title-text"
              in:fade={titleTransition.in}
              out:fade={titleTransition.out}
            >{sheetTitle}</span>
          {/key}
        </h2>
        <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
      </header>

      <div class="mobile-turn-sheet-body">
        {#if panelVisible}
          <div class="mobile-turn-sheet-panel" in:fade={subPanelIn} out:fade={subPanelOut}>
            {#if displayView === "main"}
              <p class="mobile-turn-sheet-routing-hint">{mobileComposerRoutingHint()}</p>
              <div class="mobile-turn-sheet-group">
                {#each [
                  { label: "Model", value: modelLabel, view: "provider" as const },
                  { label: "Voice", value: voiceLabel, view: "voice" as const },
                  { label: "Stance", value: depthLabel, view: "stance" as const },
                  { label: "Reasoning", value: reasoningLabel, view: "reasoning" as const },
                ] as item, index (item.label)}
                  <button
                    type="button"
                    class="mobile-turn-sheet-link-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                    disabled={navigating}
                    onclick={() => drillTo(item.view)}
                  >
                    <span class="mobile-turn-sheet-link-label">{item.label}</span>
                    <span class="mobile-turn-sheet-link-value">
                      <span class="mobile-turn-sheet-link-value-text">{item.value}</span>
                      <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                    </span>
                  </button>
                {/each}
              </div>
            {:else if displayView === "provider"}
              <label class="mobile-turn-sheet-search">
                <Search size={17} strokeWidth={2} class="mobile-turn-sheet-search-icon" />
                <input
                  type="search"
                  class="mobile-turn-sheet-search-input"
                  placeholder="Search providers"
                  aria-label="Search providers"
                  autocomplete="off"
                  autocapitalize="none"
                  spellcheck={false}
                  bind:value={providerSearch}
                />
              </label>

              {#if !providerSearch.trim() && favoriteModels.length > 0}
                <section class="mobile-turn-sheet-list-section" aria-labelledby="mobile-model-favorites">
                  <p id="mobile-model-favorites" class="mobile-turn-sheet-section-label">Favorites</p>
                  <div class="mobile-turn-sheet-group">
                    {#each favoriteModels as favorite, index (`${favorite.provider}:${favorite.model}`)}
                      {@const favoriteKey = modelPickKey(favorite.provider, favorite.model)}
                      <button
                        type="button"
                        class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                        disabled={runtime.savingControls}
                        onclick={() => void applyModel(favorite.provider, favorite.model)}
                      >
                        <span class="mobile-turn-sheet-provider-badge" aria-hidden="true">
                          {providerMonogram(favorite.provider)}
                        </span>
                        <span class="mobile-turn-sheet-row-copy">
                          <span class="mobile-turn-sheet-row-title">
                            {resolveModelDisplayLabel(favorite.provider, favorite.model)}
                          </span>
                          <span class="mobile-turn-sheet-row-subtitle">
                            {resolveProviderLabel(catalogSnapshot, favorite.provider)}
                          </span>
                        </span>
                        {#if favoriteKey === activeKey}
                          <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                        {/if}
                      </button>
                    {/each}
                  </div>
                </section>
              {/if}

              {#if loadingCatalog}
                <p class="mobile-turn-sheet-empty">Loading providers…</p>
              {:else if !catalogSnapshot}
                <p class="mobile-turn-sheet-empty">Providers could not be loaded.</p>
              {:else}
                {#if currentProviderEntry}
                  <section class="mobile-turn-sheet-list-section" aria-labelledby="mobile-current-provider">
                    <p id="mobile-current-provider" class="mobile-turn-sheet-section-label">Current provider</p>
                    <div class="mobile-turn-sheet-group">
                      <button
                        type="button"
                        class="mobile-turn-sheet-row"
                        onclick={() => void openProvider(currentProviderEntry)}
                      >
                        <span class="mobile-turn-sheet-provider-badge" aria-hidden="true">
                          {providerMonogram(currentProviderEntry.id)}
                        </span>
                        <span class="mobile-turn-sheet-row-copy">
                          <span class="mobile-turn-sheet-row-title">{currentProviderEntry.label}</span>
                          <span class="mobile-turn-sheet-row-subtitle">{modelLabel}</span>
                        </span>
                        <span class="mobile-turn-sheet-row-tail">
                          <Check size={17} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                          <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                        </span>
                      </button>
                    </div>
                  </section>
                {/if}

                {#each groupedProviders as group (group.category.id)}
                  <section class="mobile-turn-sheet-list-section" aria-labelledby={`mobile-provider-${group.category.id}`}>
                    <p id={`mobile-provider-${group.category.id}`} class="mobile-turn-sheet-section-label">
                      {group.category.label}
                    </p>
                    <div class="mobile-turn-sheet-group">
                      {#each group.providers as entry, index (entry.id)}
                        <button
                          type="button"
                          class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                          onclick={() => void openProvider(entry)}
                        >
                          <span class="mobile-turn-sheet-provider-badge" aria-hidden="true">
                            {providerMonogram(entry.id)}
                          </span>
                          <span class="mobile-turn-sheet-row-copy">
                            <span class="mobile-turn-sheet-row-title">{entry.label}</span>
                            <span class="mobile-turn-sheet-row-subtitle">{entry.blurb}</span>
                          </span>
                          <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                        </button>
                      {/each}
                    </div>
                  </section>
                {/each}

                {#if !currentProviderEntry && groupedProviders.length === 0}
                  <p class="mobile-turn-sheet-empty">No providers match that search.</p>
                {/if}
              {/if}
            {:else if displayView === "model" && selectedProvider}
              {#if providerSetupMessage}
                <div class="mobile-turn-sheet-setup">
                  <span class="mobile-turn-sheet-provider-badge" aria-hidden="true">
                    {providerMonogram(selectedProvider.id)}
                  </span>
                  <div class="mobile-turn-sheet-setup-copy">
                    <p class="mobile-turn-sheet-row-title">Set up {selectedProvider.label}</p>
                    <p class="mobile-turn-sheet-row-subtitle">{providerSetupMessage}</p>
                  </div>
                  <button type="button" class="mobile-turn-sheet-setup-action" onclick={openProviderSettings}>
                    Open settings
                  </button>
                </div>
              {:else}
                <label class="mobile-turn-sheet-search">
                  <Search size={17} strokeWidth={2} class="mobile-turn-sheet-search-icon" />
                  <input
                    type="search"
                    class="mobile-turn-sheet-search-input"
                    placeholder="Search {selectedProvider.label} models"
                    aria-label="Search models"
                    autocomplete="off"
                    autocapitalize="none"
                    spellcheck={false}
                    bind:value={modelSearch}
                  />
                </label>

                {#if modelActionError}
                  {#if localModelRequiresSetup}
                    <div class="mobile-turn-sheet-setup">
                      <p class="mobile-turn-sheet-row-subtitle" role="alert">{modelActionError}</p>
                      <button
                        type="button"
                        class="mobile-turn-sheet-setup-action"
                        onclick={openLocalBrainSettings}
                      >Open Private brain</button>
                    </div>
                  {:else}
                    <p class="mobile-turn-sheet-inline-error" role="alert">{modelActionError}</p>
                  {/if}
                {/if}

                <div class="mobile-turn-sheet-model-list">
                  {#if modelsLoading}
                    <p class="mobile-turn-sheet-empty">Loading models…</p>
                  {:else if modelLoadError}
                    <p class="mobile-turn-sheet-empty">{modelLoadError}</p>
                  {:else if filteredModels.length === 0}
                    <p class="mobile-turn-sheet-empty">
                      No catalog models match. You can enter the model ID below.
                    </p>
                  {:else}
                    <div class="mobile-turn-sheet-group" role="listbox" aria-label="Models">
                      {#each filteredModels as record, index (`${record.provider}:${record.modelId}`)}
                        {@const selected = modelPickKey(record.provider, record.modelId) === activeKey}
                        {@const needsSetup = localModelNeedsSetup(record.modelId)}
                        <button
                          type="button"
                          class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                          role="option"
                          aria-selected={selected}
                          disabled={runtime.savingControls}
                          onclick={() => void selectModel(record)}
                        >
                          <span class="mobile-turn-sheet-row-copy">
                            <span class="mobile-turn-sheet-row-title">{displayModelName(record)}</span>
                            {#if showModelSlug(record)}
                              <span class="mobile-turn-sheet-row-subtitle mobile-turn-sheet-model-slug">
                                {record.modelId}
                              </span>
                            {/if}
                          </span>
                          {#if needsSetup}
                            <span class="mobile-turn-sheet-row-tail text-[12px] font-medium text-primary-400">
                              Get in settings
                            </span>
                          {:else if selected}
                            <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                          {/if}
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>

                <div class="mobile-turn-sheet-manual">
                  <label class="mobile-turn-sheet-manual-label" for="mobile-custom-model-id">
                    Enter model ID
                  </label>
                  <div class="mobile-turn-sheet-manual-row">
                    <input
                      id="mobile-custom-model-id"
                      type="text"
                      class="mobile-turn-sheet-manual-input"
                      placeholder={selectedProvider.defaultModel || "Provider model ID"}
                      autocomplete="off"
                      autocapitalize="none"
                      spellcheck={false}
                      bind:value={manualModelId}
                      onkeydown={(event) => {
                        if (event.key === "Enter" && canUseManualModel) {
                          event.preventDefault();
                          void confirmManualModel();
                        }
                      }}
                    />
                    <button
                      type="button"
                      class="mobile-turn-sheet-manual-action"
                      disabled={!canUseManualModel}
                      onclick={() => void confirmManualModel()}
                    >Use</button>
                  </div>
                  <p class="mobile-turn-sheet-manual-hint">
                    {selectedLocalIosProvider
                      ? "Downloads and storage are managed under Settings → Connection → Private brain."
                      : "For models that do not appear in the catalog."}
                  </p>
                </div>
              {/if}
            {:else if displayView === "voice"}
              <div class="mobile-turn-sheet-group" role="listbox" aria-label="Voice">
                {#each voicePresets.allPresets as preset, index (preset.id)}
                  <button
                    type="button"
                    class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                    role="option"
                    aria-selected={voicePresets.activeVoiceId === preset.id}
                    disabled={voicePresets.saving}
                    title={preset.description}
                    onclick={() => void selectVoice(preset.id)}
                  >
                    <span class="mobile-turn-sheet-row-copy">
                      <span class="mobile-turn-sheet-row-title">{preset.name}</span>
                      {#if preset.description}<span class="mobile-turn-sheet-row-subtitle">{preset.description}</span>{/if}
                    </span>
                    {#if voicePresets.activeVoiceId === preset.id}
                      <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                {/each}
              </div>
            {:else if displayView === "stance"}
              <div class="mobile-turn-sheet-group" role="listbox" aria-label="Stance">
                {#each DEPTH_CHARTER_OPTIONS as option, index (option.id)}
                  <button
                    type="button"
                    class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                    role="option"
                    aria-selected={runtime.depthMode === option.id}
                    disabled={runtime.savingControls}
                    title={option.hint}
                    onclick={() => void selectDepth(option.id)}
                  >
                    <span class="mobile-turn-sheet-row-copy">
                      <span class="mobile-turn-sheet-row-title">{option.label}</span>
                      <span class="mobile-turn-sheet-row-subtitle">{option.hint}</span>
                    </span>
                    {#if runtime.depthMode === option.id}
                      <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                {/each}
              </div>
            {:else if displayView === "reasoning"}
              <div class="mobile-turn-sheet-group" role="listbox" aria-label="Reasoning effort">
                {#each REASONING_EFFORT_OPTIONS as option, index (option.id)}
                  <button
                    type="button"
                    class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                    role="option"
                    aria-selected={runtime.reasoningEffort === option.id}
                    disabled={runtime.savingControls}
                    title={option.hint}
                    onclick={() => void selectReasoning(option.id)}
                  >
                    <span class="mobile-turn-sheet-row-copy">
                      <span class="mobile-turn-sheet-row-title">{option.label}</span>
                      <span class="mobile-turn-sheet-row-subtitle">{option.hint}</span>
                    </span>
                    {#if runtime.reasoningEffort === option.id}
                      <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
