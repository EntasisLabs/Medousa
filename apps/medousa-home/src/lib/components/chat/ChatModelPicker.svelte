<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowUpRight,
    Check,
    ChevronDown,
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
  import { loadTuiDefaultsSummary } from "$lib/config";
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
  import type { AgentSessionConfigOption } from "$lib/daemon";
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
  const nativeMobileReadonly = $derived(readonly || isTauriMobilePlatform());
  const nativeChatGptSelected = $derived(
    selectedNativeProvider.trim().toLowerCase() === "openai-codex",
  );
  const nativeChatGptReady = $derived(chatGptOAuthReady(chatGptConnection));

  function optionTier(option: ChatModelPickOption): string | null {
    if (!option.hint || option.hint === "Active") return null;
    return option.hint;
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
          : loadTuiDefaultsSummary().catch(() => null),
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
    if (nativeMobileReadonly || !catalogSnapshot) return;
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

  function toggleMenu() {
    if (disabled || nativeMobileReadonly || runtime.savingControls) return;
    open = !open;
    if (open) {
      search = "";
      if (agentRuntime === "medousa") {
        selectedNativeProvider = runtime.provider;
        void refreshChatGptConnection();
        void refreshLiveModelsForProvider(runtime.provider);
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

  function openConnections() {
    open = false;
    settingsNav.setActiveSection("connections");
    if (layout.isMobile) layout.openMore("settings");
    else layout.navigateDesktop("settings");
  }

  function selectNativeProvider(provider: string) {
    selectedNativeProvider = provider;
    search = "";
    void refreshLiveModelsForProvider(provider);
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

  function openMenu() {
    if (disabled || runtime.savingControls) return;
    open = !open;
    if (open) {
      search = "";
      if (agentRuntime === "medousa") {
        selectedNativeProvider = runtime.provider;
        void refreshChatGptConnection();
        void refreshLiveModelsForProvider(runtime.provider);
      }
    }
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
      : ''} {nativeMobileReadonly ? 'composer-model-trigger-readonly' : ''}"
    class:composer-model-trigger-open={open}
    disabled={disabled || runtime.savingControls || agentRuntimePending}
    aria-haspopup="listbox"
    aria-expanded={open}
    title={displayName}
    onclick={nativeMobileReadonly ? openMenu : toggleMenu}
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
    <div bind:this={menuEl} class="composer-model-panel" role="dialog" aria-label="Choose model">
      {#if !nativeMobileReadonly}
        {#if agentRuntime === "medousa"}
          <div class="composer-model-provider-strip" role="listbox" aria-label="Choose provider">
            {#each nativeProviderGroups as group (group.provider)}
              <button
                type="button"
                class="composer-model-provider-option"
                class:composer-model-provider-option-active={group.provider === selectedNativeProvider}
                role="option"
                aria-selected={group.provider === selectedNativeProvider}
                onclick={() => selectNativeProvider(group.provider)}
              >
                {nativeProviderButtonLabel(group.provider, group.label)}
                {#if group.provider === "openai-codex" && !nativeChatGptReady}
                  <LogIn size={11} strokeWidth={2} />
                {/if}
              </button>
            {/each}
          </div>
          <div class="composer-model-panel-search">
            <label class="composer-model-search">
              <Search size={14} class="composer-model-search-icon" />
              <input
                type="search"
                class="composer-model-search-input"
                placeholder="Search models and providers"
                bind:value={search}
              />
            </label>
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
                <button type="button" class="composer-model-connect" onclick={openConnections}>
                  {chatGptConnection?.status === "reauth_required" ? "Reconnect" : "Open Connections"}
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
                {@const tier = optionTier(option)}
                <li>
                  <button
                    type="button"
                    class="composer-model-list-item {option.key === activeKey
                      ? 'composer-model-list-item-active'
                      : ''}"
                    role="option"
                    aria-selected={option.key === activeKey}
                    onclick={() => void selectOption(option)}
                  >
                    <span class="composer-model-row-copy">
                      <span class="composer-model-row-name">
                        {option.label}
                        {#if tier}
                          <span class="composer-model-list-tier">{tier}</span>
                        {/if}
                      </span>
                      {#if option.meta}
                        <span class="composer-model-row-meta">{option.meta}</span>
                      {/if}
                    </span>
                    {#if option.vision}
                      <span class="composer-model-row-cap">Vision</span>
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
        onclick={agentRuntime === "medousa" && !nativeChatGptSelected
          ? openModelsSettings
          : openConnections}
      >
        <span>{nativeMobileReadonly
            ? "Open Models"
            : agentRuntime === "medousa" && !nativeChatGptSelected
              ? "Manage models and providers"
              : "Manage account connection"}</span>
        <ArrowUpRight size={14} />
      </button>
    </div>
  {/if}
</div>
