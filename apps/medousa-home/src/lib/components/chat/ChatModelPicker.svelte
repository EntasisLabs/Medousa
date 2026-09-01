<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    ArrowUpRight,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Eye,
    LoaderCircle,
    LogIn,
    Search,
  } from "@lucide/svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { workshopModelOnHostHint } from "$lib/platformCopy";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildChatModelOptions,
    filterChatModelOptions,
    groupChatModelOptions,
    mergeLiveProviderModels,
    resolveProviderLabel,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { listProviderModels, listProviders, probeProviders } from "$lib/utils/providersApi";
  import {
    badgesForModel,
    capabilityMapFromCatalog,
    listModelCatalog,
    modelHasVision,
    modelMetaLine,
  } from "$lib/utils/modelCapabilityCatalog";
  import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
  import {
    normalizeFavoriteModels,
    resolveModelDisplayLabel,
    type FavoriteModel,
  } from "$lib/utils/modelCatalog";
  import { getEngineTuiDefaults, type AgentSessionConfigOption } from "$lib/daemon";
  import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";
  import {
    agentModelConfigOption,
    agentModelDisplayLabel,
    modelSourceLabel,
  } from "$lib/utils/chatModelRoute";
  import {
    chatGptOAuthReady,
    getChatGptOAuthConnection,
    listChatGptOAuthModels,
    type ChatGptOAuthConnection,
  } from "$lib/utils/chatgptOAuth";

  interface Props {
    disabled?: boolean;
    readonly?: boolean;
    /** Cursor-quiet trigger: name + chevron only. */
    quiet?: boolean;
    agentRuntime?: ChatAgentRuntime;
    agentConfigOptions?: AgentSessionConfigOption[];
    agentRuntimePending?: boolean;
    onAgentConfigChange?: (configId: string, value: unknown) => void | Promise<void>;
  }

  let {
    disabled = false,
    readonly = false,
    quiet = false,
    agentRuntime = "medousa",
    agentConfigOptions = [],
    agentRuntimePending = false,
    onAgentConfigChange,
  }: Props = $props();

  let open = $state(false);
  let search = $state("");
  let loading = $state(true);
  let options = $state<ChatModelPickOption[]>([]);
  let favorites = $state<FavoriteModel[]>([]);
  let catalogSnapshot = $state<Awaited<ReturnType<typeof listProviders>> | null>(null);
  let probeSnapshot = $state<Awaited<ReturnType<typeof probeProviders>> | null>(null);
  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let searchInputEl: HTMLInputElement | undefined = $state();
  let providerViewportEl: HTMLDivElement | undefined = $state();
  let highlightedKey = $state<string | null>(null);
  let canScrollProvidersBack = $state(false);
  let canScrollProvidersForward = $state(false);
  let selectedNativeProvider = $state(runtime.provider);
  let chatGptConnection = $state<ChatGptOAuthConnection | null>(null);
  let chatGptConnectionLoading = $state(false);
  let chatGptConnectionError = $state(false);

  let loadingLiveModels = $state(false);
  let capabilityMap = $state<Map<string, ModelCapabilityRecord>>(new Map());
  const displayName = $derived.by(() =>
    agentRuntime === "medousa"
      ? resolveModelDisplayLabel(runtime.provider, runtime.model)
      : agentModelDisplayLabel(agentRuntime, agentConfigOptions),
  );
  const externalModelOption = $derived(agentModelConfigOption(agentConfigOptions));
  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const filtered = $derived(filterChatModelOptions(options, search));
  const groupedOptions = $derived(
    groupChatModelOptions(filtered, catalogSnapshot, runtime.provider),
  );
  const visibleOptions = $derived.by(() => {
    if (search.trim()) return filtered;
    const seen = new Set<string>();
    const merged: ChatModelPickOption[] = [];
    const push = (option: ChatModelPickOption) => {
      if (seen.has(option.key)) return;
      seen.add(option.key);
      merged.push(option);
    };
    for (const option of filtered.filter((entry) => entry.favorite)) {
      push(option);
    }
    for (const group of groupedOptions) {
      for (const option of group.options) {
        push(option);
      }
    }
    return merged;
  });
  const nativeVisibleOptions = $derived(
    search.trim()
      ? visibleOptions
      : visibleOptions.filter(
          (option) =>
            option.provider.trim().toLowerCase() ===
            selectedNativeProvider.trim().toLowerCase(),
        ),
  );
  const nativeProviderGroups = $derived(
    groupedOptions.filter((group) => group.options.length > 0),
  );
  const pickerReadonly = $derived(readonly);
  const nativeChatGptSelected = $derived(
    selectedNativeProvider.trim().toLowerCase() === "openai-codex",
  );
  const nativeChatGptReady = $derived(chatGptOAuthReady(chatGptConnection));

  function optionTier(option: ChatModelPickOption): string | null {
    if (!option.hint || option.hint === "Active") return null;
    return option.hint;
  }

  function optionDetail(option: ChatModelPickOption): string | null {
    const parts: string[] = [];
    const tier = optionTier(option);
    const providerLabel = resolveProviderLabel(catalogSnapshot, option.provider);
    if (
      tier &&
      (!search.trim() || !providerLabel.toLowerCase().includes(tier.toLowerCase()))
    ) {
      parts.push(tier);
    }
    if (option.meta) {
      const providerPrefix = `${providerLabel} · `;
      const meta =
        !search.trim() && option.meta === providerLabel
          ? null
          : !search.trim() && option.meta.startsWith(providerPrefix)
            ? option.meta.slice(providerPrefix.length)
            : option.meta;
      if (meta && !parts.includes(meta)) parts.push(meta);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
  }

  onMount(() => {
    void bootstrap();
    void refreshChatGptConnection();
    const onDocClick = (event: MouseEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (menuEl?.contains(target) || triggerEl?.contains(target)) return;
      open = false;
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") open = false;
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  });

  async function refreshChatGptConnection() {
    chatGptConnectionLoading = true;
    chatGptConnectionError = false;
    try {
      chatGptConnection = await getChatGptOAuthConnection();
    } catch {
      chatGptConnection = null;
      chatGptConnectionError = true;
    } finally {
      chatGptConnectionLoading = false;
    }
  }

  $effect(() => {
    if (agentRuntime === "medousa") selectedNativeProvider = runtime.provider;
  });

  $effect(() => {
    if (!open || agentRuntime !== "medousa") return;
    if (nativeVisibleOptions.some((option) => option.key === highlightedKey)) return;
    highlightedKey =
      nativeVisibleOptions.find((option) => option.key === activeKey)?.key ??
      nativeVisibleOptions[0]?.key ??
      null;
  });

  $effect(() => {
    if (!open || agentRuntime !== "medousa") return;
    nativeProviderGroups.length;
    void tick().then(updateProviderScrollState);
  });

  function applyCapabilityData(nextOptions: ChatModelPickOption[]): ChatModelPickOption[] {
    return nextOptions.map((option) => {
      const record = capabilityMap.get(modelPickKey(option.provider, option.model));
      return {
        ...option,
        badges: badgesForModel(capabilityMap, option.provider, option.model),
        meta:
          modelMetaLine(record, resolveProviderLabel(catalogSnapshot, option.provider)) ??
          undefined,
        vision: modelHasVision(capabilityMap, option.provider, option.model),
      };
    });
  }

  async function loadCapabilityCatalog() {
    try {
      const response = await listModelCatalog();
      capabilityMap = capabilityMapFromCatalog(response.models);
      options = applyCapabilityData(options);
    } catch {
      // Curated picks still work without registry data.
    }
  }

  async function bootstrap() {
    loading = true;
    try {
      if (isTauriMobilePlatform() && !workshopDefaults.loaded) {
        await workshopDefaults.load().catch(() => {});
      }
      const [catalog, probe, summary] = await Promise.all([
        listProviders(),
        probeProviders(),
        isTauriMobilePlatform()
          ? Promise.resolve(null)
          : getEngineTuiDefaults().catch(() => null),
      ]);
      catalogSnapshot = catalog;
      probeSnapshot = probe;
      favorites = normalizeFavoriteModels(summary?.favoriteModels);
      if (workshopDefaults.loaded) {
        favorites = workshopDefaults.favoriteModels();
      }
      rebuildOptions(catalog, probe, favorites);
      await loadCapabilityCatalog();
    } catch {
      catalogSnapshot = null;
      probeSnapshot = null;
      rebuildOptions(
        {
          categories: [],
          providers: [],
        },
        null,
        favorites,
      );
    } finally {
      loading = false;
    }
  }

  function rebuildOptions(
    catalog: NonNullable<typeof catalogSnapshot>,
    probe: typeof probeSnapshot,
    nextFavorites: FavoriteModel[],
    liveModels: string[] = [],
    liveProvider = runtime.provider,
  ) {
    const base = buildChatModelOptions(
      catalog,
      probe,
      runtime.provider,
      runtime.model,
      nextFavorites,
    );
    options = liveModels.length
      ? mergeLiveProviderModels(base, liveProvider, liveModels, catalog)
      : base;
    options = applyCapabilityData(options);
  }

  async function refreshLiveModelsForProvider(provider: string) {
    if (pickerReadonly || !catalogSnapshot) return;
    loadingLiveModels = true;
    try {
      const result = provider.trim().toLowerCase() === "openai-codex"
        ? await listChatGptOAuthModels()
        : await listProviderModels({ provider });
      if (result.models.length > 0) {
        rebuildOptions(catalogSnapshot, probeSnapshot, favorites, result.models, provider);
      }
    } catch {
      // Catalog picks still work when live listing is unavailable.
    } finally {
      loadingLiveModels = false;
    }
  }

  async function toggleMenu() {
    if (disabled || pickerReadonly || runtime.savingControls) return;
    open = !open;
    if (open) {
      search = "";
      highlightedKey = activeKey;
      if (agentRuntime === "medousa") {
        selectedNativeProvider = runtime.provider;
        void refreshChatGptConnection();
        void refreshLiveModelsForProvider(runtime.provider);
        await tick();
        updateProviderScrollState();
        searchInputEl?.focus();
      }
    }
  }

  async function selectOption(option: ChatModelPickOption) {
    if (option.key === activeKey) {
      open = false;
      return;
    }
    open = false;
    await runtime.applyModel(option.provider, option.model);
  }

  function openExternalAgents() {
    open = false;
    settingsNav.setActiveSection("connections");
    if (layout.isMobile) layout.openMore("settings");
    else layout.navigateDesktop("settings");
  }

  function selectNativeProvider(provider: string, focusProvider = false) {
    selectedNativeProvider = provider;
    search = "";
    highlightedKey = null;
    void refreshLiveModelsForProvider(provider);
    void tick().then(() => {
      scrollSelectedProviderIntoView();
      if (focusProvider) {
        const buttons = providerViewportEl?.querySelectorAll<HTMLElement>("[data-provider-id]");
        Array.from(buttons ?? [])
          .find((button) => button.dataset.providerId === provider)
          ?.focus();
      } else {
        searchInputEl?.focus();
      }
    });
  }

  function updateProviderScrollState() {
    const viewport = providerViewportEl;
    if (!viewport) return;
    canScrollProvidersBack = viewport.scrollLeft > 2;
    canScrollProvidersForward =
      viewport.scrollLeft + viewport.clientWidth < viewport.scrollWidth - 2;
  }

  function scrollProviderRail(direction: -1 | 1) {
    const viewport = providerViewportEl;
    if (!viewport) return;
    viewport.scrollBy({
      left: direction * Math.max(140, viewport.clientWidth * 0.72),
      behavior: "smooth",
    });
  }

  function scrollSelectedProviderIntoView() {
    const buttons = providerViewportEl?.querySelectorAll<HTMLElement>("[data-provider-id]");
    const active = Array.from(buttons ?? []).find(
      (button) => button.dataset.providerId === selectedNativeProvider,
    );
    active?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
    window.setTimeout(updateProviderScrollState, 180);
  }

  function selectAdjacentProvider(direction: -1 | 1, focusProvider = false) {
    if (nativeProviderGroups.length === 0) return;
    const currentIndex = nativeProviderGroups.findIndex(
      (group) => group.provider === selectedNativeProvider,
    );
    const nextIndex = Math.min(
      nativeProviderGroups.length - 1,
      Math.max(0, (currentIndex < 0 ? 0 : currentIndex) + direction),
    );
    const provider = nativeProviderGroups[nextIndex]?.provider;
    if (provider && provider !== selectedNativeProvider) {
      selectNativeProvider(provider, focusProvider);
    }
  }

  function nativeProviderButtonLabel(provider: string, label: string): string {
    return provider.trim().toLowerCase() === "openai" ? "OpenAI · API key" : label;
  }

  async function selectExternalModel(value: unknown) {
    const option = externalModelOption;
    if (!option || value === option.currentValue) {
      open = false;
      return;
    }
    await onAgentConfigChange?.(option.id, value);
    open = false;
  }

  async function openMenu() {
    if (disabled || runtime.savingControls) return;
    open = !open;
    if (open) {
      search = "";
      highlightedKey = activeKey;
      if (agentRuntime === "medousa") {
        selectedNativeProvider = runtime.provider;
        void refreshChatGptConnection();
        void refreshLiveModelsForProvider(runtime.provider);
        await tick();
        updateProviderScrollState();
        searchInputEl?.focus();
      }
    }
  }

  function scrollHighlightedIntoView() {
    void tick().then(() => {
      const items = menuEl?.querySelectorAll<HTMLElement>("[data-model-key]");
      const item = Array.from(items ?? []).find(
        (candidate) => candidate.dataset.modelKey === highlightedKey,
      );
      item?.scrollIntoView({ block: "nearest" });
    });
  }

  function handlePickerKeydown(event: KeyboardEvent) {
    if (agentRuntime !== "medousa") return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open = false;
      triggerEl?.focus();
      return;
    }
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      const target = event.target as HTMLElement | null;
      const navigatingProviders =
        target === searchInputEl ? search.length === 0 : target?.dataset.providerId != null;
      if (navigatingProviders) {
        event.preventDefault();
        selectAdjacentProvider(
          event.key === "ArrowRight" ? 1 : -1,
          target?.dataset.providerId != null,
        );
      }
      return;
    }
    if (event.key === "Enter" && highlightedKey) {
      const option = nativeVisibleOptions.find((entry) => entry.key === highlightedKey);
      if (!option) return;
      event.preventDefault();
      void selectOption(option);
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if (nativeVisibleOptions.length === 0) return;
    event.preventDefault();
    const currentIndex = nativeVisibleOptions.findIndex(
      (option) => option.key === highlightedKey,
    );
    const direction = event.key === "ArrowDown" ? 1 : -1;
    const nextIndex =
      currentIndex < 0
        ? direction > 0
          ? 0
          : nativeVisibleOptions.length - 1
        : (currentIndex + direction + nativeVisibleOptions.length) %
          nativeVisibleOptions.length;
    highlightedKey = nativeVisibleOptions[nextIndex]?.key ?? null;
    scrollHighlightedIntoView();
  }

  function openModelsSettings() {
    settingsNav.openSection("agent");
    if (layout.isMobile) {
      layout.openMore("settings");
      return;
    }
    layout.navigateDesktop("settings");
  }
</script>

<div class="composer-model-picker" class:composer-model-picker-quiet={quiet}>
  <button
    bind:this={triggerEl}
    type="button"
    class="composer-model-trigger {quiet
      ? 'composer-model-trigger--quiet'
      : ''} {pickerReadonly ? 'composer-model-trigger-readonly' : ''}"
    class:composer-model-trigger-open={open}
    disabled={disabled || runtime.savingControls || agentRuntimePending}
    aria-haspopup="listbox"
    aria-expanded={open}
    title={displayName}
    onclick={pickerReadonly ? openMenu : toggleMenu}
  >
    <span class="composer-model-trigger-copy">
      <span class="composer-model-trigger-name">{displayName}</span>
    </span>
    {#if runtime.savingControls || agentRuntimePending}
      <LoaderCircle size={13} class="composer-model-trigger-spinner animate-spin" />
    {:else}
      <ChevronDown size={13} class="composer-model-trigger-chevron" />
    {/if}
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="composer-model-panel"
      role="dialog"
      aria-label="Choose model"
      tabindex="-1"
      onkeydown={handlePickerKeydown}
    >
      {#if !pickerReadonly}
        {#if agentRuntime === "medousa"}
          <div class="composer-model-panel-search">
            <label class="composer-model-search">
              <Search size={15} class="composer-model-search-icon" />
              <input
                bind:this={searchInputEl}
                type="search"
                class="composer-model-search-input"
                placeholder="Search models and providers…"
                bind:value={search}
                aria-label="Search models and providers"
              />
            </label>
          </div>
          <div class="composer-model-provider-nav">
            <div
              bind:this={providerViewportEl}
              class="composer-model-provider-strip"
              role="listbox"
              aria-label="Choose provider"
              onscroll={updateProviderScrollState}
            >
              {#each nativeProviderGroups as group (group.provider)}
                <button
                  type="button"
                  class="composer-model-provider-option"
                  class:composer-model-provider-option-active={group.provider === selectedNativeProvider}
                  role="option"
                  aria-selected={group.provider === selectedNativeProvider}
                  data-provider-id={group.provider}
                  onclick={() => selectNativeProvider(group.provider)}
                >
                  {nativeProviderButtonLabel(group.provider, group.label)}
                  {#if group.provider === "openai-codex" && !nativeChatGptReady}
                    <LogIn size={11} strokeWidth={2} />
                  {/if}
                </button>
              {/each}
            </div>
            <div class="composer-model-provider-pager" aria-label="Scroll providers">
              <button
                type="button"
                class="composer-model-provider-page"
                disabled={!canScrollProvidersBack}
                aria-label="Previous providers"
                onclick={() => scrollProviderRail(-1)}
              >
                <ChevronLeft size={13} strokeWidth={1.8} />
              </button>
              <button
                type="button"
                class="composer-model-provider-page"
                disabled={!canScrollProvidersForward}
                aria-label="Next providers"
                onclick={() => scrollProviderRail(1)}
              >
                <ChevronRight size={13} strokeWidth={1.8} />
              </button>
            </div>
          </div>
          <ul class="composer-model-list" role="listbox">
            {#if nativeChatGptSelected && chatGptConnectionLoading}
              <li class="composer-model-list-empty">
                <LoaderCircle size={16} class="animate-spin opacity-60" />
                <span>Checking ChatGPT account…</span>
              </li>
            {:else if nativeChatGptSelected && !nativeChatGptReady}
              <li class="composer-model-list-empty">
                <LogIn size={17} strokeWidth={1.85} />
                <span>{chatGptConnectionError
                    ? "Could not verify the ChatGPT connection"
                    : chatGptConnection?.status === "reauth_required"
                      ? "Reconnect your ChatGPT account to continue"
                      : "Connect a ChatGPT account to use subscription models"}</span>
                <button type="button" class="composer-model-connect" onclick={openModelsSettings}>
                  {chatGptConnection?.status === "reauth_required"
                    ? "Reconnect"
                    : "Open Medousa Agent"}
                </button>
              </li>
            {:else if loading || loadingLiveModels}
              <li class="composer-model-list-empty">
                <LoaderCircle size={16} class="animate-spin opacity-60" />
                <span>{loading ? "Loading models…" : "Refreshing models…"}</span>
              </li>
            {:else if nativeVisibleOptions.length === 0}
              <li class="composer-model-list-empty">No models found for this provider</li>
            {:else}
              {#each nativeVisibleOptions as option (option.key)}
                {@const detail = optionDetail(option)}
                <li>
                  <button
                    type="button"
                    class="composer-model-list-item {option.key === activeKey
                      ? 'composer-model-list-item-active'
                      : ''} {option.key === highlightedKey
                      ? 'composer-model-list-item-highlighted'
                      : ''}"
                    role="option"
                    aria-selected={option.key === activeKey}
                    data-model-key={option.key}
                    onmouseenter={() => (highlightedKey = option.key)}
                    onclick={() => void selectOption(option)}
                  >
                    <span class="composer-model-row-copy">
                      <span class="composer-model-row-name">{option.label}</span>
                      {#if detail}
                        <span class="composer-model-row-meta">{detail}</span>
                      {/if}
                    </span>
                    {#if option.vision}
                      <span class="composer-model-row-cap"><Eye size={11} /> Vision</span>
                    {/if}
                    {#if option.key === activeKey}
                      <Check size={15} strokeWidth={2.5} class="composer-model-list-check" />
                    {/if}
                  </button>
                </li>
              {/each}
            {/if}
          </ul>
        {:else}
          <ul class="composer-model-list" role="listbox">
            {#if agentRuntimePending}
              <li class="composer-model-list-empty">
                <LoaderCircle size={16} class="animate-spin opacity-60" />
                <span>Connecting {modelSourceLabel(agentRuntime)}…</span>
              </li>
            {:else if !externalModelOption}
              <li class="composer-model-list-empty">
                This runtime did not advertise model choices for this session.
              </li>
            {:else}
              {#each externalModelOption.options ?? [] as choice, index (`${choice.name}:${index}`)}
                {@const selected = choice.value === externalModelOption.currentValue}
                <li>
                  <button
                    type="button"
                    class="composer-model-list-item {selected ? 'composer-model-list-item-active' : ''}"
                    role="option"
                    aria-selected={selected}
                    onclick={() => void selectExternalModel(choice.value)}
                  >
                    <span class="composer-model-row-copy">
                      <span class="composer-model-row-name">{choice.name}</span>
                      {#if choice.description}
                        <span class="composer-model-row-meta">{choice.description}</span>
                      {/if}
                    </span>
                    {#if selected}
                      <Check size={15} strokeWidth={2.5} class="composer-model-list-check" />
                    {/if}
                  </button>
                </li>
              {/each}
            {/if}
          </ul>
        {/if}
      {:else}
        <div class="composer-model-mobile-note">
          <p class="composer-model-mobile-title">{runtime.modelLabel()}</p>
          <p class="composer-model-mobile-copy">{workshopModelOnHostHint()}</p>
        </div>
      {/if}

      <button
        type="button"
        class="composer-model-panel-footer"
        onclick={agentRuntime === "medousa" ? openModelsSettings : openExternalAgents}
      >
        <span>{pickerReadonly
            ? "Open Models"
            : agentRuntime === "medousa"
              ? "Manage models and providers"
              : "Manage external agent"}</span>
        <ArrowUpRight size={14} />
      </button>
    </div>
  {/if}
</div>
