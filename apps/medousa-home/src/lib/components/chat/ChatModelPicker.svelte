<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowUpRight, Check, ChevronDown, LoaderCircle, Search } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { workshopModelOnHostHint } from "$lib/platformCopy";
  import { loadTuiDefaultsSummary } from "$lib/config";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildChatModelOptions,
    depthModeLabel,
    filterChatModelOptions,
    groupNonFavoriteChatModelOptions,
    mergeLiveProviderModels,
    resolveProviderLabel,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { listProviderModels, listProviders, probeProviders } from "$lib/utils/providersApi";
  import {
    badgesForModel,
    capabilityMapFromCatalog,
    listModelCatalog,
  } from "$lib/utils/modelCapabilityCatalog";
  import type { ModelCapabilityRecord } from "$lib/types/modelCapability";
  import ModelCapabilityBadges from "$lib/components/settings/ModelCapabilityBadges.svelte";
  import {
    normalizeFavoriteModels,
    resolveModelDisplayLabel,
    type FavoriteModel,
  } from "$lib/utils/modelCatalog";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import {
    REASONING_EFFORT_OPTIONS,
    reasoningEffortLabel,
  } from "$lib/types/reasoningEffort";
  import { allVoicePresets } from "$lib/types/voicePresets";
  import type { DepthMode, ReasoningEffortMode } from "$lib/types/runtime";

  interface Props {
    disabled?: boolean;
    readonly?: boolean;
  }

  let { disabled = false, readonly = false }: Props = $props();

  let open = $state(false);
  let search = $state("");
  let loading = $state(true);
  let options = $state<ChatModelPickOption[]>([]);
  let favorites = $state<FavoriteModel[]>([]);
  let catalogSnapshot = $state<Awaited<ReturnType<typeof listProviders>> | null>(null);
  let probeSnapshot = $state<Awaited<ReturnType<typeof probeProviders>> | null>(null);
  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();

  let loadingLiveModels = $state(false);
  let capabilityMap = $state<Map<string, ModelCapabilityRecord>>(new Map());

  const displayName = $derived(resolveModelDisplayLabel(runtime.provider, runtime.model));
  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const filtered = $derived(filterChatModelOptions(options, search));
  const groupedOptions = $derived(
    groupNonFavoriteChatModelOptions(filtered, catalogSnapshot, runtime.provider),
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
  const nativeMobileReadonly = $derived(readonly || isTauriMobilePlatform());
  const depthLabel = $derived(depthModeLabel(runtime.depthMode));
  const reasoningLabel = $derived(reasoningEffortLabel(runtime.reasoningEffort));
  const voiceLabel = $derived(voicePresets.activePreset.name);
  const voiceOptions = $derived(allVoicePresets(workshopDefaults.draft.customVoicePresets));

  function optionProviderLabel(option: ChatModelPickOption): string | null {
    const provider = option.provider.trim().toLowerCase();
    if (provider === runtime.provider.trim().toLowerCase()) return null;
    return resolveProviderLabel(catalogSnapshot, provider);
  }

  onMount(() => {
    void bootstrap();
    void voicePresets.load();
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

  function applyCapabilityBadges(nextOptions: ChatModelPickOption[]): ChatModelPickOption[] {
    if (capabilityMap.size === 0) return nextOptions;
    return nextOptions.map((option) => ({
      ...option,
      badges: badgesForModel(capabilityMap, option.provider, option.model),
    }));
  }

  async function loadCapabilityCatalog() {
    try {
      const response = await listModelCatalog();
      capabilityMap = capabilityMapFromCatalog(response.models);
      options = applyCapabilityBadges(options);
    } catch {
      // Curated picks still work without registry data.
    }
  }

  async function bootstrap() {
    loading = true;
    try {
      const [catalog, probe, summary] = await Promise.all([
        listProviders(),
        probeProviders(),
        loadTuiDefaultsSummary().catch(() => null),
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
  ) {
    const base = buildChatModelOptions(
      catalog,
      probe,
      runtime.provider,
      runtime.model,
      nextFavorites,
    );
    options = liveModels.length
      ? mergeLiveProviderModels(base, runtime.provider, liveModels, catalog)
      : base;
    options = applyCapabilityBadges(options);
  }

  async function refreshLiveModelsForActiveProvider() {
    if (nativeMobileReadonly || !catalogSnapshot) return;
    loadingLiveModels = true;
    try {
      const result = await listProviderModels({ provider: runtime.provider });
      if (result.models.length > 0) {
        rebuildOptions(catalogSnapshot, probeSnapshot, favorites, result.models);
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
      void refreshLiveModelsForActiveProvider();
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

  async function selectDepth(mode: DepthMode) {
    if (mode === runtime.depthMode || runtime.savingControls) return;
    await runtime.setDepthMode(mode);
  }

  async function selectReasoning(mode: ReasoningEffortMode) {
    if (mode === runtime.reasoningEffort || runtime.savingControls) return;
    await runtime.setReasoningEffort(mode);
  }

  async function selectVoice(voiceId: string) {
    if (voiceId === voicePresets.activeVoiceId || voicePresets.saving) return;
    await voicePresets.setActiveVoiceId(voiceId);
  }

  function openMenu() {
    if (disabled || runtime.savingControls) return;
    open = !open;
    if (open) {
      search = "";
      void refreshLiveModelsForActiveProvider();
    }
  }

  function openModelsSettings() {
    settingsNav.openSection("models");
    if (layout.isMobile) {
      layout.openYou("settings");
      return;
    }
    layout.navigateDesktop("settings");
  }
</script>

<div class="composer-model-picker">
  <button
    bind:this={triggerEl}
    type="button"
    class="composer-model-trigger {nativeMobileReadonly ? 'composer-model-trigger-readonly' : ''}"
    class:composer-model-trigger-open={open}
    disabled={disabled || runtime.savingControls}
    aria-haspopup="listbox"
    aria-expanded={open}
    title="{displayName} · {voiceLabel} · {depthLabel} · {reasoningLabel}"
    onclick={nativeMobileReadonly ? openMenu : toggleMenu}
  >
    <span class="composer-model-trigger-copy">
      <span class="composer-model-trigger-name">{displayName}</span>
      <span class="composer-model-trigger-meta">{voiceLabel} · {depthLabel} · {reasoningLabel}</span>
    </span>
    {#if runtime.savingControls}
      <LoaderCircle size={13} class="composer-model-trigger-spinner animate-spin" />
    {:else}
      <ChevronDown size={13} class="composer-model-trigger-chevron" />
    {/if}
  </button>

  {#if open}
    <div bind:this={menuEl} class="composer-model-panel" role="dialog" aria-label="Choose model">
      {#if !nativeMobileReadonly}
        <div class="composer-model-panel-search">
          <label class="composer-model-search">
            <Search size={14} class="composer-model-search-icon" />
            <input
              type="search"
              class="composer-model-search-input"
              placeholder="Search models"
              bind:value={search}
            />
          </label>
        </div>

        <ul class="composer-model-list" role="listbox">
          {#if loading || loadingLiveModels}
            <li class="composer-model-list-empty">
              <LoaderCircle size={16} class="animate-spin opacity-60" />
              <span>{loading ? "Loading models…" : "Refreshing models…"}</span>
            </li>
          {:else if visibleOptions.length === 0}
            <li class="composer-model-list-empty">No matches</li>
          {:else}
            {#each visibleOptions as option (option.key)}
              {@const providerLabel = optionProviderLabel(option)}
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
                  <span class="composer-model-list-name">
                    {option.label}
                    {#if providerLabel}
                      <span class="composer-model-list-tier">{providerLabel}</span>
                    {:else if option.hint && option.hint !== "Active"}
                      <span class="composer-model-list-tier">{option.hint}</span>
                    {/if}
                    <ModelCapabilityBadges badges={option.badges ?? []} compact />
                  </span>
                  {#if option.key === activeKey}
                    <Check size={15} strokeWidth={2.5} class="composer-model-list-check" />
                  {/if}
                </button>
              </li>
            {/each}
          {/if}
        </ul>

        <div class="composer-model-turn-settings" aria-label="Turn settings">
          <div class="composer-model-turn-row">
            <span class="composer-model-turn-label">Voice</span>
            <div class="composer-model-turn-pills" role="group" aria-label="Voice">
              {#each voiceOptions as option (option.id)}
                <button
                  type="button"
                  class="composer-model-turn-pill {voicePresets.activeVoiceId === option.id
                    ? 'composer-model-turn-pill-active'
                    : ''}"
                  disabled={voicePresets.saving}
                  aria-pressed={voicePresets.activeVoiceId === option.id}
                  title={option.description}
                  onclick={() => void selectVoice(option.id)}
                >
                  {option.name}
                </button>
              {/each}
            </div>
          </div>

          <div class="composer-model-turn-row">
            <span class="composer-model-turn-label">Stance</span>
            <div class="composer-model-turn-pills" role="group" aria-label="Answer depth">
              {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
                <button
                  type="button"
                  class="composer-model-turn-pill {runtime.depthMode === option.id
                    ? 'composer-model-turn-pill-active'
                    : ''}"
                  disabled={runtime.savingControls}
                  aria-pressed={runtime.depthMode === option.id}
                  title={option.hint}
                  onclick={() => void selectDepth(option.id)}
                >
                  {option.label}
                </button>
              {/each}
            </div>
          </div>

          <div class="composer-model-turn-row">
            <span class="composer-model-turn-label">Reasoning</span>
            <select
              class="composer-model-turn-select"
              value={runtime.reasoningEffort}
              disabled={runtime.savingControls}
              aria-label="Reasoning effort"
              onchange={(event) =>
                void selectReasoning(
                  (event.currentTarget as HTMLSelectElement).value as ReasoningEffortMode,
                )}
            >
              {#each REASONING_EFFORT_OPTIONS as option (option.id)}
                <option value={option.id} title={option.hint}>{option.label}</option>
              {/each}
            </select>
          </div>
        </div>
      {:else}
        <div class="composer-model-mobile-note">
          <p class="composer-model-mobile-title">{runtime.modelLabel()}</p>
          <p class="composer-model-mobile-copy">{workshopModelOnHostHint()}</p>
        </div>
      {/if}

      <button type="button" class="composer-model-panel-footer" onclick={openModelsSettings}>
        <span>{nativeMobileReadonly ? "Open Models" : "Add models"}</span>
        <ArrowUpRight size={14} />
      </button>
    </div>
  {/if}
</div>
