<script lang="ts">
  import { onMount, tick } from "svelte";
  import { AudioLines, Brain, Check, ChevronDown, Compass } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { depthModeLabel } from "$lib/utils/chatModelPicker";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import {
    REASONING_EFFORT_OPTIONS,
    reasoningEffortLabel,
  } from "$lib/types/reasoningEffort";
  import { allVoicePresets } from "$lib/types/voicePresets";
  import type { DepthMode, ReasoningEffortMode } from "$lib/types/runtime";

  type TurnMenu = "voice" | "stance" | "reasoning";

  interface Props {
    disabled?: boolean;
    showNativeControls?: boolean;
  }

  let { disabled = false, showNativeControls = true }: Props = $props();

  let openMenu = $state<TurnMenu | null>(null);
  let rootEl = $state<HTMLDivElement | null>(null);
  let voiceTriggerEl = $state<HTMLButtonElement | null>(null);
  let stanceTriggerEl = $state<HTMLButtonElement | null>(null);
  let reasoningTriggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const voiceLabel = $derived(voicePresets.activePreset.name);
  const depthLabel = $derived(depthModeLabel(runtime.depthMode));
  const reasoningLabel = $derived(reasoningEffortLabel(runtime.reasoningEffort));
  const voiceOptions = $derived(allVoicePresets(workshopDefaults.draft.customVoicePresets));
  const pickerDisabled = $derived(disabled || runtime.savingControls || voicePresets.saving);

  const activeTrigger = $derived(
    openMenu === "voice"
      ? voiceTriggerEl
      : openMenu === "stance"
        ? stanceTriggerEl
        : openMenu === "reasoning"
          ? reasoningTriggerEl
          : null,
  );

  onMount(() => {
    void voicePresets.load();
  });

  $effect(() => {
    if (!openMenu || !menuEl || !activeTrigger) return;

    let frame = 0;
    const place = () => {
      if (!menuEl || !activeTrigger) return;
      placeComposerPopover(activeTrigger, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (menuEl && activeTrigger) placeComposerPopover(activeTrigger, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);

    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(rootEl?.contains(target) || menuEl?.contains(target)),
      onDismiss: () => {
        openMenu = null;
      },
    });

    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  function toggle(menu: TurnMenu) {
    if (pickerDisabled) return;
    openMenu = openMenu === menu ? null : menu;
  }

  async function selectVoice(voiceId: string) {
    if (voiceId === voicePresets.activeVoiceId || voicePresets.saving) {
      openMenu = null;
      return;
    }
    await voicePresets.setActiveVoiceId(voiceId);
    if (workshopDefaults.loaded) {
      workshopDefaults.draft = {
        ...workshopDefaults.draft,
        activeVoiceId: voiceId,
      };
    }
    openMenu = null;
  }

  async function selectDepth(mode: DepthMode) {
    if (mode === runtime.depthMode || runtime.savingControls) {
      openMenu = null;
      return;
    }
    await runtime.setDepthMode(mode);
    openMenu = null;
  }

  async function selectReasoning(mode: ReasoningEffortMode) {
    if (mode === runtime.reasoningEffort || runtime.savingControls) {
      openMenu = null;
      return;
    }
    await runtime.setReasoningEffort(mode);
    openMenu = null;
  }
</script>

<div bind:this={rootEl} class="composer-turn-controls">
  <button
    bind:this={voiceTriggerEl}
    type="button"
    class="composer-turn-trigger"
    class:composer-turn-trigger-open={openMenu === "voice"}
    disabled={pickerDisabled}
    aria-haspopup="listbox"
    aria-expanded={openMenu === "voice"}
    aria-label="Voice — {voiceLabel}"
    title="Voice — {voiceLabel}"
    onclick={() => toggle("voice")}
  >
    <AudioLines size={13} strokeWidth={1.85} class="composer-turn-trigger-icon" />
    <span class="composer-turn-trigger-label">{voiceLabel}</span>
    <ChevronDown size={12} strokeWidth={2} class="composer-turn-trigger-chevron shrink-0" />
  </button>

  {#if showNativeControls}
    <button
      bind:this={stanceTriggerEl}
      type="button"
      class="composer-turn-trigger"
      class:composer-turn-trigger-open={openMenu === "stance"}
      disabled={pickerDisabled}
      aria-haspopup="listbox"
      aria-expanded={openMenu === "stance"}
      aria-label="Stance — {depthLabel}"
      title="Stance — {depthLabel}"
      onclick={() => toggle("stance")}
    >
      <Compass size={13} strokeWidth={1.85} class="composer-turn-trigger-icon" />
      <span class="composer-turn-trigger-label">{depthLabel}</span>
      <ChevronDown size={12} strokeWidth={2} class="composer-turn-trigger-chevron shrink-0" />
    </button>

    <button
      bind:this={reasoningTriggerEl}
      type="button"
      class="composer-turn-trigger"
      class:composer-turn-trigger-open={openMenu === "reasoning"}
      disabled={pickerDisabled}
      aria-haspopup="listbox"
      aria-expanded={openMenu === "reasoning"}
      aria-label="Reasoning — {reasoningLabel}"
      title="Reasoning — {reasoningLabel}"
      onclick={() => toggle("reasoning")}
    >
      <Brain size={13} strokeWidth={1.85} class="composer-turn-trigger-icon" />
      <span class="composer-turn-trigger-label">{reasoningLabel}</span>
      <ChevronDown size={12} strokeWidth={2} class="composer-turn-trigger-chevron shrink-0" />
    </button>
  {/if}

  {#if openMenu === "voice"}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="composer-anchored-menu composer-turn-menu"
        role="listbox"
        aria-label="Choose voice"
      >
        <header class="composer-anchored-menu-header">
          <h2 class="text-sm font-semibold text-surface-50">Voice</h2>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each voiceOptions as option (option.id)}
            {@const active = voicePresets.activeVoiceId === option.id}
            <button
              type="button"
              class="composer-turn-option"
              class:composer-turn-option-active={active}
              role="option"
              aria-selected={active}
              title={option.description}
              onclick={() => void selectVoice(option.id)}
            >
              <span class="composer-turn-option-label">{option.name}</span>
              {#if active}
                <Check size={14} strokeWidth={2} class="composer-turn-option-check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </BodyPortal>
  {:else if showNativeControls && openMenu === "stance"}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="composer-anchored-menu composer-turn-menu"
        role="listbox"
        aria-label="Choose stance"
      >
        <header class="composer-anchored-menu-header">
          <h2 class="text-sm font-semibold text-surface-50">Stance</h2>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each DEPTH_CHARTER_OPTIONS as option (option.id)}
            {@const active = runtime.depthMode === option.id}
            <button
              type="button"
              class="composer-turn-option"
              class:composer-turn-option-active={active}
              role="option"
              aria-selected={active}
              title={option.hint}
              onclick={() => void selectDepth(option.id)}
            >
              <span class="composer-turn-option-label">{option.label}</span>
              {#if active}
                <Check size={14} strokeWidth={2} class="composer-turn-option-check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </BodyPortal>
  {:else if showNativeControls && openMenu === "reasoning"}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="composer-anchored-menu composer-turn-menu"
        role="listbox"
        aria-label="Choose reasoning"
      >
        <header class="composer-anchored-menu-header">
          <h2 class="text-sm font-semibold text-surface-50">Reasoning</h2>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each REASONING_EFFORT_OPTIONS as option (option.id)}
            {@const active = runtime.reasoningEffort === option.id}
            <button
              type="button"
              class="composer-turn-option"
              class:composer-turn-option-active={active}
              role="option"
              aria-selected={active}
              title={option.hint}
              onclick={() => void selectReasoning(option.id)}
            >
              <span class="composer-turn-option-label">{option.label}</span>
              {#if active}
                <Check size={14} strokeWidth={2} class="composer-turn-option-check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </BodyPortal>
  {/if}
</div>
