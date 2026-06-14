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
</script>

<div class="composer-model-picker">
  {#if nativeMobileReadonly}
    <span class="composer-model-pill composer-model-pill-readonly" title={runtime.modelLabel()}>
      {displayName}
    </span>
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

    {#if open}
      <div bind:this={menuEl} class="composer-model-menu" role="listbox">
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

        {#if onOpenVoiceSettings}
          <button type="button" class="composer-model-menu-footer" onclick={onOpenVoiceSettings}>
            All models in Workshop
          </button>
        {/if}
      </div>
    {/if}
  {/if}
</div>
