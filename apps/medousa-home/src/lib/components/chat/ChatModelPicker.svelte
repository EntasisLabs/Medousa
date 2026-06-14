<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowUpRight, Check, ChevronDown, LoaderCircle, Search, Sparkles, Star } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { loadTuiDefaultsSummary, persistTuiFavoriteModels } from "$lib/config";
  import { formatModelDisplayName, modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildChatModelOptions,
    depthModeLabel,
    filterChatModelOptions,
    providerMonogram,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { listProviders, probeProviders } from "$lib/utils/providersApi";
  import {
    isFavoriteModel,
    normalizeFavoriteModels,
    toggleFavoriteModel,
    type FavoriteModel,
  } from "$lib/utils/modelCatalog";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import type { DepthMode } from "$lib/types/runtime";

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

  const displayName = $derived(formatModelDisplayName(runtime.model));
  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const filtered = $derived(filterChatModelOptions(options, search));
  const favoriteOptions = $derived(filtered.filter((option) => option.favorite));
  const otherOptions = $derived(filtered.filter((option) => !option.favorite));
  const activeIsFavorite = $derived(isFavoriteModel(favorites, runtime.provider, runtime.model));
  const nativeMobileReadonly = $derived(readonly || isTauriMobilePlatform());
  const providerBadge = $derived(providerMonogram(runtime.provider));
  const depthLabel = $derived(depthModeLabel(runtime.depthMode));

  onMount(() => {
    void bootstrap();
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
  ) {
    options = buildChatModelOptions(
      catalog,
      probe,
      runtime.provider,
      runtime.model,
      nextFavorites,
    );
  }

  async function toggleActiveFavorite(event: MouseEvent) {
    event.stopPropagation();
    if (nativeMobileReadonly || runtime.savingControls) return;
    const next = toggleFavoriteModel(favorites, runtime.provider, runtime.model);
    favorites = next;
    workshopDefaults.draft = { ...workshopDefaults.draft, favoriteModels: next };
    if (catalogSnapshot) {
      rebuildOptions(catalogSnapshot, probeSnapshot, next);
    }
    try {
      await persistTuiFavoriteModels(next);
    } catch {
      // Favorites still update locally; persist retries on next settings save.
    }
  }

  function toggleMenu() {
    if (disabled || nativeMobileReadonly || runtime.savingControls) return;
    open = !open;
    if (open) search = "";
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

  function openMenu() {
    if (disabled || runtime.savingControls) return;
    open = !open;
    if (open) search = "";
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
    title="{runtime.modelLabel()} · {depthLabel} depth"
    onclick={nativeMobileReadonly ? openMenu : toggleMenu}
  >
    <span class="composer-model-trigger-badge" aria-hidden="true">{providerBadge}</span>
    <span class="composer-model-trigger-copy">
      <span class="composer-model-trigger-name">{displayName}</span>
      <span class="composer-model-trigger-meta">{depthLabel}</span>
    </span>
    {#if runtime.savingControls}
      <LoaderCircle size={13} class="composer-model-trigger-spinner animate-spin" />
    {:else}
      <ChevronDown size={13} class="composer-model-trigger-chevron" />
    {/if}
  </button>

  {#if open}
    <div bind:this={menuEl} class="composer-model-panel" role="dialog" aria-label="Model picker">
      <div class="composer-model-panel-header">
        <div class="composer-model-panel-title">
          <Sparkles size={14} class="composer-model-panel-icon" />
          <span>Model</span>
        </div>
        <div class="composer-model-panel-header-actions">
          {#if !nativeMobileReadonly}
            <button
              type="button"
              class="composer-model-favorite-btn {activeIsFavorite ? 'is-active' : ''}"
              disabled={runtime.savingControls}
              aria-pressed={activeIsFavorite}
              title={activeIsFavorite ? "Remove from favorites" : "Add to favorites"}
              onclick={(event) => void toggleActiveFavorite(event)}
            >
              <Star size={14} fill={activeIsFavorite ? "currentColor" : "none"} />
            </button>
          {/if}
          <span class="composer-model-panel-active">{displayName}</span>
        </div>
      </div>

      <div class="composer-model-panel-section">
        <span class="composer-model-panel-label">Answer depth</span>
        <div class="composer-model-depth-segment" role="group" aria-label="Answer depth">
          {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
            <button
              type="button"
              class="composer-model-depth-segment-btn {runtime.depthMode === option.id
                ? 'composer-model-depth-segment-btn-active'
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

      {#if !nativeMobileReadonly}
        <div class="composer-model-panel-section composer-model-panel-section-search">
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
          {#if loading}
            <li class="composer-model-list-empty">
              <LoaderCircle size={16} class="animate-spin opacity-60" />
              <span>Loading models…</span>
            </li>
          {:else if filtered.length === 0}
            <li class="composer-model-list-empty">No matches</li>
          {:else}
            {#if favoriteOptions.length > 0 && !search.trim()}
              <li class="composer-model-list-section" aria-hidden="true">Favorites</li>
              {#each favoriteOptions as option (option.key)}
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
                    <span class="composer-model-list-badge">{providerMonogram(option.provider)}</span>
                    <span class="composer-model-list-copy">
                      <span class="composer-model-list-name">{option.label}</span>
                      {#if option.hint}
                        <span class="composer-model-list-hint">{option.hint}</span>
                      {/if}
                    </span>
                    {#if option.key === activeKey}
                      <span class="composer-model-list-check" aria-hidden="true">
                        <Check size={14} strokeWidth={2.75} />
                      </span>
                    {/if}
                  </button>
                </li>
              {/each}
              {#if otherOptions.length > 0}
                <li class="composer-model-list-section" aria-hidden="true">More models</li>
              {/if}
            {/if}
            {#each (search.trim() ? filtered : otherOptions) as option (option.key)}
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
                  <span class="composer-model-list-badge">{providerMonogram(option.provider)}</span>
                  <span class="composer-model-list-copy">
                    <span class="composer-model-list-name">{option.label}</span>
                    {#if option.hint}
                      <span class="composer-model-list-hint">{option.hint}</span>
                    {/if}
                  </span>
                  {#if option.key === activeKey}
                    <span class="composer-model-list-check" aria-hidden="true">
                      <Check size={14} strokeWidth={2.75} />
                    </span>
                  {/if}
                </button>
              </li>
            {/each}
          {/if}
        </ul>
      {:else}
        <div class="composer-model-mobile-note">
          <p class="composer-model-mobile-title">{runtime.modelLabel()}</p>
          <p class="composer-model-mobile-copy">Model is set on your Mac workshop</p>
        </div>
      {/if}

      <button type="button" class="composer-model-panel-footer" onclick={openModelsSettings}>
        <span>{nativeMobileReadonly ? "Open Models" : "Models in Settings"}</span>
        <ArrowUpRight size={14} />
      </button>
    </div>
  {/if}
</div>
