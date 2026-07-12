<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicIn, cubicOut } from "svelte/easing";
  import { Check, ChevronDown, ChevronLeft, ChevronRight, X } from "@lucide/svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { loadTuiDefaultsSummary } from "$lib/config";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { modelPickKey } from "$lib/utils/formatModelDisplay";
  import {
    buildMobileModelDropdownOptions,
    groupChatModelOptions,
    type ChatModelPickOption,
  } from "$lib/utils/chatModelPicker";
  import { normalizeFavoriteModels, resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
  import { listProviders, probeProviders } from "$lib/utils/providersApi";
  import { mobileComposerRoutingHint } from "$lib/platformCopy";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import { haptic } from "$lib/haptics";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import type { DepthMode, ReasoningEffortMode } from "$lib/types/runtime";
  import {
    REASONING_EFFORT_OPTIONS,
    reasoningEffortLabel,
  } from "$lib/types/reasoningEffort";

  type SheetView = "main" | "voice" | "stance" | "reasoning";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  let open = $state(false);
  let sheetView = $state<SheetView>("main");
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let loading = $state(true);
  let options = $state<ChatModelPickOption[]>([]);
  let catalogSnapshot = $state<Awaited<ReturnType<typeof listProviders>> | null>(null);

  const activeKey = $derived(modelPickKey(runtime.provider, runtime.model));
  const groupedOptions = $derived(
    groupChatModelOptions(options, catalogSnapshot, runtime.provider),
  );
  const modelLabel = $derived(resolveModelDisplayLabel(runtime.provider, runtime.model));
  const voiceLabel = $derived(voicePresets.activePreset.name);
  const depthLabel = $derived(
    DEPTH_CHARTER_OPTIONS.find((option) => option.id === runtime.depthMode)?.label ?? "Standard",
  );
  const reasoningLabel = $derived(reasoningEffortLabel(runtime.reasoningEffort));
  const pickerDisabled = $derived(
    disabled || runtime.savingControls || voicePresets.saving,
  );
  const sheetTitle = $derived(
    sheetView === "main"
      ? "Select model"
      : sheetView === "voice"
        ? "Voice"
        : sheetView === "stance"
          ? "Stance"
          : "Reasoning",
  );
  const backdropTransition = {
    in: { duration: 260, easing: cubicOut },
    out: { duration: 200, easing: cubicIn },
  };
  const sheetTransition = {
    in: { y: 420, duration: 360, easing: cubicOut, opacity: 1 },
    out: { y: 280, duration: 260, easing: cubicIn, opacity: 1 },
  };
  const titleTransition = {
    in: { duration: 150, easing: cubicOut },
    out: { duration: 100, easing: cubicIn },
  };
  const subPanelOut = { duration: 120, easing: cubicIn };
  const subPanelIn = { duration: 150, easing: cubicOut };
  const SUB_PANEL_CLEAR_MS = 130;

  let displayView = $state<SheetView>("main");
  let panelVisible = $state(true);
  let navigating = $state(false);

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
      let favorites = normalizeFavoriteModels(summary?.favoriteModels);
      if (workshopDefaults.loaded) {
        favorites = workshopDefaults.favoriteModels();
      }
      catalogSnapshot = catalog;
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

  function openSheet() {
    if (pickerDisabled) return;
    displayView = "main";
    sheetView = "main";
    panelVisible = true;
    navigating = false;
    open = true;
  }

  function closeSheet() {
    open = false;
    displayView = "main";
    sheetView = "main";
    panelVisible = true;
    navigating = false;
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

  function drillTo(view: Exclude<SheetView, "main">) {
    void transitionToView(view);
  }

  function goBack() {
    void transitionToView("main");
  }

  function handleSheetSwipeBack(): boolean {
    if (sheetView === "main") return false;
    void transitionToView("main");
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

  async function selectModel(option: ChatModelPickOption) {
    if (option.key === activeKey || runtime.savingControls) return;
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

<div class="mobile-composer-turn">
  <button
    type="button"
    class="mobile-composer-turn-trigger {open ? 'mobile-composer-turn-trigger-open' : ''}"
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label="Model and turn settings: {modelLabel}, {depthLabel} stance, {voiceLabel} voice"
    disabled={pickerDisabled}
    onclick={openSheet}
  >
    <span class="mobile-composer-turn-trigger-label">
      {loading ? "Model" : modelLabel}
      <span class="mobile-composer-turn-trigger-sep" aria-hidden="true">·</span>
      {depthLabel}
    </span>
    <ChevronDown size={13} class="mobile-composer-turn-trigger-chevron" />
  </button>
</div>

{#if open}
  <div
    class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
    role="presentation"
    in:fade={backdropTransition.in}
    out:fade={backdropTransition.out}
    onclick={(event) => {
      if (event.target === event.currentTarget) closeSheet();
    }}
  >
      <div
        bind:this={sheetEl}
        class="mobile-sheet mobile-turn-sheet"
        role="dialog"
        aria-label={sheetTitle}
        tabindex="-1"
        in:fly={sheetTransition.in}
        out:fly={sheetTransition.out}
        onclick={(event) => event.stopPropagation()}
        onkeydown={handleSheetKeydown}
      >
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>

        <header bind:this={headerEl} class="mobile-turn-sheet-header">
          {#if sheetView === "main"}
            <button
              type="button"
              class="mobile-turn-sheet-icon-btn"
              aria-label="Close"
              onclick={closeSheet}
            >
              <X size={18} strokeWidth={2} />
            </button>
          {:else}
            <button
              type="button"
              class="mobile-turn-sheet-icon-btn"
              aria-label="Back"
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
              >
                {sheetTitle}
              </span>
            {/key}
          </h2>

          <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
        </header>

        <div class="mobile-turn-sheet-body">
          {#if panelVisible}
            <div class="mobile-turn-sheet-panel" in:fade={subPanelIn} out:fade={subPanelOut}>
              {#if displayView === "main"}
                <p class="mobile-turn-sheet-routing-hint">{mobileComposerRoutingHint()}</p>
                {#if loading}
                  <p class="mobile-turn-sheet-empty">Loading models…</p>
                {:else if options.length === 0}
                  <p class="mobile-turn-sheet-empty">No pinned models yet.</p>
                {:else}
                  <div class="mobile-turn-sheet-group" role="listbox" aria-label="Model">
                    {#each groupedOptions as group, groupIndex (group.provider)}
                      {#if groupIndex > 0}
                        <div class="mobile-turn-sheet-section-gap" aria-hidden="true"></div>
                      {/if}
                      <p class="mobile-turn-sheet-section-label">{group.label}</p>
                      {#each group.options as option, index (option.key)}
                        <button
                          type="button"
                          class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
                          role="option"
                          aria-selected={option.key === activeKey}
                          disabled={runtime.savingControls}
                          onclick={() => void selectModel(option)}
                        >
                          <span class="mobile-turn-sheet-row-copy">
                            <span class="mobile-turn-sheet-row-title">{option.label}</span>
                            {#if option.hint}
                              <span class="mobile-turn-sheet-row-subtitle">{option.hint}</span>
                            {/if}
                          </span>
                          {#if option.key === activeKey}
                            <Check size={18} strokeWidth={2.5} class="mobile-turn-sheet-row-check" />
                          {/if}
                        </button>
                      {/each}
                    {/each}
                  </div>
                {/if}

                <div class="mobile-turn-sheet-group mobile-turn-sheet-group-secondary">
                  <button
                    type="button"
                    class="mobile-turn-sheet-link-row"
                    disabled={navigating}
                    onclick={() => drillTo("voice")}
                  >
                    <span class="mobile-turn-sheet-link-label">Voice</span>
                    <span class="mobile-turn-sheet-link-value">
                      {voiceLabel}
                      <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                    </span>
                  </button>
                  <button
                    type="button"
                    class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
                    disabled={navigating}
                    onclick={() => drillTo("stance")}
                  >
                    <span class="mobile-turn-sheet-link-label">Stance</span>
                    <span class="mobile-turn-sheet-link-value">
                      {depthLabel}
                      <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                    </span>
                  </button>
                  <button
                    type="button"
                    class="mobile-turn-sheet-link-row mobile-turn-sheet-row-divider"
                    disabled={navigating}
                    onclick={() => drillTo("reasoning")}
                  >
                    <span class="mobile-turn-sheet-link-label">Reasoning</span>
                    <span class="mobile-turn-sheet-link-value">
                      {reasoningLabel}
                      <ChevronRight size={16} strokeWidth={2} class="mobile-turn-sheet-link-chevron" />
                    </span>
                  </button>
                </div>
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
                        {#if preset.description}
                          <span class="mobile-turn-sheet-row-subtitle">{preset.description}</span>
                        {/if}
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
              {:else}
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
