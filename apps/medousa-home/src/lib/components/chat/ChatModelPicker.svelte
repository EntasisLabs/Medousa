<script lang="ts">
  import { onMount } from "svelte";
  import { Check, ChevronDown, LoaderCircle, Search } from "@lucide/svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { formatModelDisplayName, modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildChatModelOptions,
    filterChatModelOptions,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { listProviders, probeProviders } from "$lib/utils/providersApi";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import type { DepthMode } from "$lib/types/runtime";

  interface Props {
    disabled?: boolean;
    readonly?: boolean;
    onOpenVoiceSettings?: () => void;
  }

  let { disabled = false, readonly = false, onOpenVoiceSettings }: Props = $props();

  let open = $state(false);
  let search = $state("");
  let loading = $state(true);
  let options = $state<ChatModelPickOption[]>([]);
  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();

  const displayName = $derived(formatModelDisplayName(runtime.model));
  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const filtered = $derived(filterChatModelOptions(options, search));
  const nativeMobileReadonly = $derived(readonly || isTauriMobilePlatform());

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
      const [catalog, probe] = await Promise.all([listProviders(), probeProviders()]);
      options = buildChatModelOptions(catalog, probe, runtime.provider, runtime.model);
    } catch {
      options = buildChatModelOptions(
        {
          categories: [],
          providers: [],
        },
        null,
        runtime.provider,
        runtime.model,
      );
    } finally {
      loading = false;
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
</script>

<div class="composer-model-picker">
  {#if nativeMobileReadonly}
    <button
      bind:this={triggerEl}
      type="button"
      class="composer-model-pill composer-model-pill-readonly"
      title="{runtime.modelLabel()} · depth {runtime.depthMode}"
      disabled={disabled}
      aria-haspopup="menu"
      aria-expanded={open}
      onclick={openMenu}
    >
      <span class="truncate">{displayName}</span>
      <ChevronDown size={12} class="shrink-0 opacity-70" />
    </button>
  {:else}
    <button
      bind:this={triggerEl}
      type="button"
      class="composer-model-pill"
      disabled={disabled || runtime.savingControls}
      aria-haspopup="listbox"
      aria-expanded={open}
      onclick={toggleMenu}
    >
      {#if runtime.savingControls}
        <LoaderCircle size={12} class="animate-spin opacity-70" />
      {/if}
      <span class="truncate">{displayName}</span>
      <ChevronDown size={12} class="shrink-0 opacity-70" />
    </button>
  {/if}

  {#if open}
    <div bind:this={menuEl} class="composer-model-menu" role="listbox">
      <div class="composer-model-menu-depth">
        <span class="composer-model-menu-depth-label">Depth</span>
        <div class="composer-model-menu-depth-row">
          {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
            <button
              type="button"
              class="composer-model-depth-pill {runtime.depthMode === option.id
                ? 'composer-model-depth-pill-active'
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
        <div class="composer-model-menu-search">
          <Search size={14} class="shrink-0 opacity-60" />
          <input
            type="search"
            class="composer-model-menu-search-input"
            placeholder="Search models"
            bind:value={search}
          />
        </div>

        <ul class="composer-model-menu-list">
          {#if loading}
            <li class="composer-model-menu-empty">Loading models…</li>
          {:else if filtered.length === 0}
            <li class="composer-model-menu-empty">No matches</li>
          {:else}
            {#each filtered as option (option.key)}
              <li>
                <button
                  type="button"
                  class="composer-model-menu-item"
                  role="option"
                  aria-selected={option.key === activeKey}
                  onclick={() => void selectOption(option)}
                >
                  <span class="min-w-0 flex-1 truncate text-left">
                    <span class="block truncate text-sm text-surface-100">{option.label}</span>
                    {#if option.hint}
                      <span class="block truncate text-[10px] text-surface-500">{option.hint}</span>
                    {/if}
                  </span>
                  {#if option.key === activeKey}
                    <Check size={14} class="shrink-0 text-primary-300" />
                  {/if}
                </button>
              </li>
            {/each}
          {/if}
        </ul>
      {:else}
        <div class="composer-model-menu-mobile-note">
          <p class="text-xs text-surface-300">{runtime.modelLabel()}</p>
          <p class="mt-1 text-[11px] text-surface-500">Model is set on your Mac workshop</p>
        </div>
      {/if}

      {#if onOpenVoiceSettings}
        <button type="button" class="composer-model-menu-footer" onclick={onOpenVoiceSettings}>
          {nativeMobileReadonly ? "Open Workshop" : "All models in Workshop"}
        </button>
      {/if}
    </div>
  {/if}
</div>
