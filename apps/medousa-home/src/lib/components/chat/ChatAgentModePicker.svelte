<script lang="ts">
  import { tick } from "svelte";
  import { Check, ChevronDown, Code2, Sparkles } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { getSessionAgentMode, listAgentModes, setSessionAgentMode } from "$lib/daemon";
  import type { AgentModeId } from "$lib/types/session";
  import type { AgentModeAvailability } from "$lib/types/generated/daemon_api";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    sessionId: string;
    disabled?: boolean;
  }

  let { sessionId, disabled = false }: Props = $props();

  const FALLBACK_MODES: AgentModeAvailability[] = [
    {
      mode: "general",
      label: "General",
      available: true,
      contract_revision: "general-v1",
    },
    {
      mode: "coder",
      label: "Coder",
      available: false,
      unavailable_reason: "Coder readiness could not be verified",
    },
  ];

  let modes = $state<AgentModeAvailability[]>(FALLBACK_MODES);
  let value = $state<AgentModeId>("general");
  let open = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let loadRevision = 0;
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const active = $derived(modes.find((mode) => mode.mode === value) ?? modes[0]);
  const label = $derived(active?.label ?? "General");

  $effect(() => {
    const nextSessionId = sessionId.trim();
    const revision = ++loadRevision;
    value = "general";
    error = null;
    if (!nextSessionId) return;
    void refresh(nextSessionId, revision);
  });

  async function refresh(nextSessionId: string, revision: number) {
    try {
      const [registry, state] = await Promise.all([
        listAgentModes(),
        getSessionAgentMode(nextSessionId),
      ]);
      if (revision !== loadRevision) return;
      modes = registry.modes.length > 0 ? registry.modes : FALLBACK_MODES;
      value = state.effective_mode;
    } catch (err) {
      if (revision !== loadRevision) return;
      modes = FALLBACK_MODES;
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function pick(mode: AgentModeAvailability) {
    if (!mode.available || mode.mode === value || loading) {
      if (mode.mode === value) open = false;
      return;
    }
    loading = true;
    error = null;
    try {
      const state = await setSessionAgentMode(sessionId, mode.mode);
      value = state.effective_mode;
      open = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (!open || !menuEl || !triggerEl) return;
    let frame = 0;
    const place = () => {
      if (!menuEl || !triggerEl) return;
      placeComposerPopover(triggerEl, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (menuEl && triggerEl) placeComposerPopover(triggerEl, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) => Boolean(menuEl?.contains(target) || triggerEl?.contains(target)),
      onDismiss: () => (open = false),
    });
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });
</script>

{#snippet modeIcon(mode: AgentModeId, size = 13)}
  {#if mode === "coder"}
    <Code2 {size} strokeWidth={1.9} class="shrink-0 opacity-75" />
  {:else}
    <Sparkles {size} strokeWidth={1.9} class="shrink-0 opacity-75" />
  {/if}
{/snippet}

<div class="chat-runtime-picker">
  <button
    bind:this={triggerEl}
    type="button"
    class="chat-runtime-trigger"
    class:chat-runtime-trigger-open={open}
    disabled={disabled || loading}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label="Medousa mode — {label}"
    title="How Medousa works in this chat"
    onclick={() => {
      if (!disabled && !loading) open = !open;
    }}
  >
    {@render modeIcon(value)}
    <span class="chat-runtime-trigger-label">{label}</span>
    <ChevronDown size={12} strokeWidth={2} class="chat-runtime-trigger-chevron shrink-0" />
  </button>

  {#if open}
    <BodyPortal>
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        bind:this={menuEl}
        class="composer-anchored-menu chat-runtime-menu"
        role="listbox"
        aria-label="Choose Medousa mode"
      >
        <header class="composer-anchored-menu-header">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Mode</h2>
            <p class="workshop-faint mt-0.5 text-xs">How Medousa approaches this chat</p>
          </div>
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          {#each modes as mode (mode.mode)}
            <button
              type="button"
              class="chat-runtime-option"
              class:chat-runtime-option-active={value === mode.mode}
              class:chat-runtime-option-locked={!mode.available}
              role="option"
              aria-selected={value === mode.mode}
              aria-disabled={!mode.available}
              disabled={!mode.available || loading}
              onclick={() => void pick(mode)}
            >
              {@render modeIcon(mode.mode, 14)}
              <span class="min-w-0 flex-1 text-left">
                <span class="block text-[13px] font-medium text-surface-100">{mode.label}</span>
                <span class="workshop-faint mt-0.5 block text-[11px]">
                  {mode.available
                    ? mode.mode === "general"
                      ? "Life, planning, research, and everyday work"
                      : "Repository-aware engineering"
                    : mode.unavailable_reason ?? "Not ready on this workshop"}
                </span>
              </span>
              {#if value === mode.mode}
                <Check size={14} strokeWidth={2} class="shrink-0 text-primary-300" />
              {:else if !mode.available}
                <span class="shrink-0 text-[10px] font-semibold uppercase tracking-wide text-surface-400">
                  Soon
                </span>
              {/if}
            </button>
          {/each}
          {#if error}
            <p class="px-2 py-1 text-[11px] text-error-300" role="status">
              Mode state unavailable. General remains active.
            </p>
          {/if}
        </div>
      </div>
    </BodyPortal>
  {/if}
</div>
