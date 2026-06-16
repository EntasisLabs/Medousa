<script lang="ts">
  import { onMount } from "svelte";
  import { Check, ChevronDown, Sparkles } from "@lucide/svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { loadTuiDefaultsSummary } from "$lib/config";
  import { isCompanionShell, workshopModelOnHostHint } from "$lib/platformCopy";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildMobileModelDropdownOptions,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
  import { listProviders, probeProviders } from "$lib/utils/providersApi";
  import { normalizeFavoriteModels } from "$lib/utils/modelCatalog";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import type { DepthMode } from "$lib/types/runtime";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  let open = $state(false);
  let loading = $state(true);
  let options = $state<ChatModelPickOption[]>([]);
  let triggerEl: HTMLButtonElement | undefined = $state();
  let popoverEl: HTMLDivElement | undefined = $state();

  const companion = $derived(isCompanionShell());
  const modelReadonly = $derived(companion);
  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const modelLabel = $derived(resolveModelDisplayLabel(runtime.provider, runtime.model));
  const voiceLabel = $derived(voicePresets.activePreset.name);
  const depthLabel = $derived(
    DEPTH_CHARTER_OPTIONS.find((option) => option.id === runtime.depthMode)?.label ?? "Standard",
  );
  const triggerMeta = $derived(`${voiceLabel} · ${depthLabel}`);
  const pickerDisabled = $derived(
    disabled || runtime.savingControls || voicePresets.saving,
  );

  onMount(() => {
    void bootstrap();
    void voicePresets.load();

    const onDocClick = (event: MouseEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (popoverEl?.contains(target) || triggerEl?.contains(target)) return;
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
      let favorites = normalizeFavoriteModels(summary?.favoriteModels);
      if (workshopDefaults.loaded) {
        favorites = workshopDefaults.favoriteModels();
      }
      options = buildMobileModelDropdownOptions(
        catalog,
        probe,
        runtime.provider,
        runtime.model,
        favorites,
      );
    } catch {
      options = [];
    } finally {
      loading = false;
    }
  }

  function toggleOpen() {
    if (pickerDisabled) return;
    open = !open;
  }

  async function selectModel(option: ChatModelPickOption) {
    if (modelReadonly || option.key === activeKey || runtime.savingControls) return;
    await runtime.applyModel(option.provider, option.model);
  }

  async function selectVoice(voiceId: string) {
    if (voiceId === voicePresets.activeVoiceId || voicePresets.saving) return;
    await voicePresets.setActiveVoiceId(voiceId);
  }

  async function selectDepth(mode: DepthMode) {
    if (mode === runtime.depthMode || runtime.savingControls) return;
    await runtime.setDepthMode(mode);
  }
</script>

<div class="mobile-composer-settings">
  <button
    bind:this={triggerEl}
    type="button"
    class="mobile-composer-settings-trigger {open ? 'mobile-composer-settings-trigger-open' : ''}"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label="Turn settings: {modelLabel}, {voiceLabel}, {depthLabel} depth"
    disabled={pickerDisabled}
    onclick={toggleOpen}
  >
    <span class="mobile-composer-settings-trigger-copy">
      <span class="mobile-composer-settings-trigger-name">{loading ? "Model" : modelLabel}</span>
      <span class="mobile-composer-settings-trigger-meta">{triggerMeta}</span>
    </span>
    <ChevronDown size={14} class="mobile-composer-settings-trigger-chevron" />
  </button>

  {#if open}
    <button
      type="button"
      class="mobile-composer-settings-backdrop"
      aria-label="Close turn settings"
      onclick={() => (open = false)}
    ></button>

    <div
      bind:this={popoverEl}
      class="mobile-composer-settings-popover"
      role="dialog"
      aria-label="Turn settings"
    >
      <header class="mobile-composer-settings-header">
        <div class="mobile-composer-settings-title">
          <Sparkles size={14} />
          <span>Turn settings</span>
        </div>
        <span class="mobile-composer-settings-active">{modelLabel}</span>
      </header>

      <section class="mobile-composer-settings-section">
        <span class="mobile-composer-settings-label">Model</span>
        {#if modelReadonly}
          <p class="mobile-composer-settings-readonly">{modelLabel}</p>
          <p class="mobile-composer-settings-footnote">{workshopModelOnHostHint()}</p>
        {:else if loading}
          <p class="mobile-composer-settings-readonly">Loading…</p>
        {:else if options.length === 0}
          <p class="mobile-composer-settings-footnote">No pinned models yet.</p>
        {:else}
          <ul class="mobile-composer-settings-list" role="listbox" aria-label="Model">
            {#each options as option (option.key)}
              <li>
                <button
                  type="button"
                  class="mobile-composer-settings-option {option.key === activeKey
                    ? 'mobile-composer-settings-option-active'
                    : ''}"
                  role="option"
                  aria-selected={option.key === activeKey}
                  disabled={runtime.savingControls}
                  onclick={() => void selectModel(option)}
                >
                  <span class="mobile-composer-settings-option-copy">
                    <span class="mobile-composer-settings-option-name">{option.label}</span>
                    {#if option.hint}
                      <span class="mobile-composer-settings-option-hint">{option.hint}</span>
                    {/if}
                  </span>
                  {#if option.key === activeKey}
                    <Check size={14} strokeWidth={2.75} aria-hidden="true" />
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section class="mobile-composer-settings-section">
        <span class="mobile-composer-settings-label">Voice</span>
        <div class="mobile-composer-settings-segment" role="group" aria-label="Voice">
          {#each voicePresets.allPresets as preset (preset.id)}
            <button
              type="button"
              class="mobile-composer-settings-segment-btn {voicePresets.activeVoiceId === preset.id
                ? 'mobile-composer-settings-segment-btn-active'
                : ''}"
              disabled={voicePresets.saving}
              aria-pressed={voicePresets.activeVoiceId === preset.id}
              title={preset.description}
              onclick={() => void selectVoice(preset.id)}
            >
              {preset.name}
            </button>
          {/each}
        </div>
      </section>

      <section class="mobile-composer-settings-section">
        <span class="mobile-composer-settings-label">Stance</span>
        <div class="mobile-composer-settings-segment" role="group" aria-label="Answer depth">
          {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
            <button
              type="button"
              class="mobile-composer-settings-segment-btn {runtime.depthMode === option.id
                ? 'mobile-composer-settings-segment-btn-active'
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
      </section>
    </div>
  {/if}
</div>
