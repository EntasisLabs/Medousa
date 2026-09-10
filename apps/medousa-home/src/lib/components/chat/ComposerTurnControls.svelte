<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { composerModel } from "$lib/chat/composerModel";
  import ReasoningOptions from "$lib/components/chat/ReasoningOptions.svelte";
  import { composerReasoning, selectComposerReasoning } from "$lib/chat/composerModel";
  import { trackReasoningCapabilities } from "$lib/chat/reasoningCapabilities.svelte";

  import { onMount, tick, type Snippet } from "svelte";
  import { ArrowLeft, Check, ChevronRight, SlidersHorizontal, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import ChatNarrationToggle from "$lib/components/chat/ChatNarrationToggle.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";
  import { DEPTH_CHARTER_OPTIONS } from "$lib/types/settings";
  import { reasoningEffortLabel } from "$lib/types/reasoningEffort";
  import { allVoicePresets } from "$lib/types/voicePresets";
  import type { DepthMode } from "$lib/types/runtime";

  const reasoning = $derived(composerReasoning());
  type View = "main" | "voice" | "depth" | "reasoning" | "agent" | "drafts";
  interface Props {
    disabled?: boolean;
    sessionId: string;
    showNativeControls?: boolean;
    runtimeLabel: string;
    agentSettings: Snippet;
    drafts: Snippet<[() => void]>;
  }
  let {
    disabled = false,
    sessionId,
    showNativeControls = true,
    runtimeLabel,
    agentSettings,
    drafts,
  }: Props = $props();
  let open = $state(false);
  let view = $state<View>("main");
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  const pickerDisabled = $derived(disabled || runtime.savingControls || voicePresets.saving);
  const voiceOptions = $derived(allVoicePresets(workshopDefaults.draft.customVoicePresets));
  const depthLabel = $derived(
    DEPTH_CHARTER_OPTIONS.find((option) => option.id === runtime.depthMode)?.label ?? "Standard",
  );
  const titles: Record<View, string> = {
    main: "Turn settings",
    voice: "Response style",
    depth: "Response depth",
    reasoning: "Reasoning",
    agent: "Agent runtime",
    drafts: "Saved drafts",
  };
  const title = $derived(titles[view]);
  const choices = $derived(
    view === "voice" ? voiceOptions.map((option) => ({ id: option.id, label: option.name, hint: option.description }))
      : DEPTH_CHARTER_OPTIONS,
  );
  const selected = $derived(view === "voice" ? voicePresets.activeVoiceId : view === "depth" ? runtime.depthMode : reasoning.value);

  onMount(() => {
    void voicePresets.load();
  });

  $effect(() => {
    sessionId;
    open = false;
    view = "main";
  });

  function close() {
    open = false;
    triggerEl?.focus();
  }
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key !== "Tab" || !menuEl) return;
    const controls = [...menuEl.querySelectorAll<HTMLElement>(
      "button:not(:disabled), select:not(:disabled), input:not(:disabled)",
    )];
    const first = controls[0];
    const last = controls.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
  async function navigate(next: View) {
    view = next;
    await tick();
    menuEl?.querySelector<HTMLButtonElement>("button")?.focus();
  }
  async function select(id: string) {
    if (pickerDisabled) return;
    if (view === "voice") {
      await voicePresets.setActiveVoiceId(id);
      if (workshopDefaults.loaded) {
        workshopDefaults.draft = { ...workshopDefaults.draft, activeVoiceId: id };
      }
    } else if (view === "depth") {
      await runtime.setDepthMode(id as DepthMode);
    } else if (view === "reasoning") {
      selectComposerReasoning(id);
    }
    await navigate("main");
  }

  $effect(() => {
    if (!open || !menuEl || !triggerEl) return;
    // Reposition as subviews change size without imposing a fixed-height panel.
    const place = () => {
      if (menuEl && triggerEl) {
        placeComposerPopover(triggerEl, menuEl, { maxHeightRatio: 0.68 });
      }
    };
    const observer = new ResizeObserver(place);
    observer.observe(menuEl);
    place();
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    const detach = attachComposerMenuDismiss({
      isInside: (target) => Boolean(menuEl?.contains(target) || triggerEl?.contains(target)),
      onDismiss: () => { open = false; },
    });
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      detach();
    };
  });
  trackReasoningCapabilities(() => ({ ...composerModel(), scope: chat.workshopScopeId, refresh: open && view === "reasoning" }));
</script>

<button bind:this={triggerEl} type="button" class="chat-runtime-trigger composer-settings-trigger"
  aria-label="Turn settings" title="Turn settings" aria-haspopup="dialog" aria-expanded={open}
  disabled={pickerDisabled}
  onclick={async () => {
    open = !open;
    view = "main";
    if (open) await navigate("main");
  }}
>
  <SlidersHorizontal size={14} strokeWidth={1.8} />
  <span>Settings</span>
</button>

{#snippet settingRow(label: string, value: string, next: View)}
  <button type="button" class="composer-settings-row" onclick={() => void navigate(next)}>
    <span>{label}</span>
    <span class="composer-settings-value">{value}</span>
    <ChevronRight size={14} />
  </button>
{/snippet}

{#if open}
  <BodyPortal>
    <div bind:this={menuEl} class="composer-anchored-menu composer-settings-menu" role="dialog" tabindex="-1" aria-label={title} onkeydown={handleKeydown}>
      <header class="composer-settings-header">
        {#if view !== "main"}
          <button type="button" aria-label="Back to turn settings" onclick={() => void navigate("main")}>
            <ArrowLeft size={15} />
          </button>
        {/if}
        <h2>{title}</h2>
        <button type="button" aria-label="Close turn settings" onclick={close}><X size={15} /></button>
      </header>
      <div class="composer-anchored-menu-body">
        {#if view === "main"}
          {#if showNativeControls}
            {@render settingRow("Response style", voicePresets.activePreset.name, "voice")}
            {@render settingRow("Response depth", depthLabel, "depth")}
            {@render settingRow("Reasoning", reasoningEffortLabel(reasoning.value), "reasoning")}
          {/if}
          <div class="composer-settings-audio">
            <span>Read replies aloud</span>
            <ChatNarrationToggle />
          </div>
          <div class="composer-settings-divider"></div>
          {@render settingRow("Agent runtime", runtimeLabel, "agent")}
          {@render settingRow("Saved drafts", "", "drafts")}
        {:else if view === "agent"}
          {@render agentSettings()}
        {:else if view === "drafts"}
          {@render drafts(close)}
        {:else if view === "reasoning"}
          <ReasoningOptions {...reasoning} disabled={pickerDisabled} onchange={(value) => { selectComposerReasoning(value); void navigate("main"); }} />
        {:else}
          <div role="group" aria-label={title}>
            {#each choices as option (option.id)}
              <button type="button" class="composer-turn-option" aria-pressed={selected === option.id}
                disabled={pickerDisabled} onclick={() => void select(option.id)}>
                <span class="composer-turn-option-copy"><span class="composer-turn-option-label">{option.label}</span>
                  <span class="composer-turn-option-description">{option.hint}</span></span>
                {#if selected === option.id}<Check size={14} class="composer-turn-option-check" />{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .composer-settings-trigger {
    flex-shrink: 0;
  }

  .composer-settings-menu {
    width: min(20rem, calc(100vw - 1rem));
  }

  .composer-settings-header {
    display: flex;
    align-items: center;
    gap: .5rem;
    padding: .65rem .7rem;
    border-bottom: 1px solid rgb(var(--theme-border) / .25);
  }

  .composer-settings-header h2 {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
  }

  .composer-settings-header button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: .4rem;
    color: rgb(var(--theme-text-secondary));
  }

  .composer-settings-header button:hover {
    background: rgb(var(--theme-border) / .2);
  }

  .composer-settings-row, .composer-settings-audio {
    display: flex;
    width: 100%;
    align-items: center;
    gap: .6rem;
    min-height: 42px;
    padding: .5rem .6rem;
    font-size: 12px;
    text-align: left;
    border-radius: .5rem;
  }

  .composer-settings-row:hover {
    background: rgb(var(--theme-border) / .15);
  }

  .composer-settings-value {
    margin-left: auto;
    max-width: 48%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgb(var(--theme-text-tertiary));
  }

  .composer-settings-audio {
    justify-content: space-between;
  }

  .composer-settings-divider {
    margin: .35rem .6rem;
    border-top: 1px solid rgb(var(--theme-border) / .25);
  }

  .composer-settings-menu :global(button:focus-visible) {
    outline: 2px solid currentColor;
    outline-offset: -2px;
  }

</style>
