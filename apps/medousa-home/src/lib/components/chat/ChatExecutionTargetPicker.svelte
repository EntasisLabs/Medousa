<script lang="ts">
  import { tick } from "svelte";
  import { Check, ChevronDown, Cpu, WandSparkles } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { layout } from "$lib/runtime/layout.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import type {
    ExecutionTargetInventoryEntry,
    ExecutionTargetSelection,
  } from "$lib/daemon/runtime";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    sessionId: string;
    disabled?: boolean;
  }

  let { sessionId, disabled = false }: Props = $props();
  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let sheetHeaderEl = $state<HTMLElement | null>(null);

  const selection = $derived(executionTargets.selectionFor(sessionId));
  const userTargets = $derived(executionTargets.userTargets());
  const agentTargets = $derived(executionTargets.agentTargets());
  const parentTarget = $derived(executionTargets.parentTarget());
  const label = $derived(executionTargets.selectionLabel(sessionId));
  const unavailable = $derived(executionTargets.selectionUnavailable(sessionId));
  const visible = $derived(executionTargets.shouldShow(sessionId));
  const defaultLabel = $derived(
    executionTargets.runtimeLabel(executionTargets.defaultRuntimeId()) ?? "current default",
  );

  $effect(() => {
    const id = sessionId.trim();
    open = false;
    if (!id) return;
    void executionTargets.refresh().catch(() => undefined);
  });

  function sameSelection(candidate: ExecutionTargetSelection | null): boolean {
    if (!candidate || !selection) return candidate === selection;
    if (candidate.kind !== selection.kind) return false;
    if (candidate.kind === "exact" && selection.kind === "exact") {
      return candidate.runtime_id === selection.runtime_id;
    }
    return true;
  }

  function choose(candidate: ExecutionTargetSelection | null) {
    if (disabled) return;
    executionTargets.setSelection(sessionId, candidate);
    haptic("light");
    open = false;
  }

  function toggle() {
    if (disabled) return;
    open = !open;
    if (open) void executionTargets.refresh({ force: true }).catch(() => undefined);
  }

  function close() {
    open = false;
  }

  function targetDescription(target: ExecutionTargetInventoryEntry): string {
    const environment = [target.platform, target.architecture].filter(Boolean).join(" · ");
    const authority = target.agent_selectable
      ? "Available to you and Medousa"
      : "Runs here only when you choose it";
    return environment ? `${authority} · ${environment}` : authority;
  }

  $effect(() => {
    if (layout.isMobile || !open || !menuEl || !triggerEl) return;
    let frame = 0;
    const place = () => {
      if (!menuEl || !triggerEl) return;
      placeComposerPopover(triggerEl, menuEl, { maxHeightRatio: 0.66 });
      frame = window.requestAnimationFrame(() => {
        if (menuEl && triggerEl) {
          placeComposerPopover(triggerEl, menuEl, { maxHeightRatio: 0.66 });
        }
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) => Boolean(menuEl?.contains(target) || triggerEl?.contains(target)),
      onDismiss: close,
    });
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  $effect(() => {
    if (!layout.isMobile || !open || !sheetEl) return;
    const detachGestures = attachMobileSheetGestures(sheetEl, sheetHeaderEl, {
      onDismiss: close,
    });
    const detachBack = registerMobileBackHandler(() => {
      close();
      return true;
    });
    return () => {
      detachGestures();
      detachBack();
    };
  });
</script>

{#snippet checkmark(active: boolean)}
  {#if active}
    <Check size={14} strokeWidth={2} class="shrink-0 text-content-link" />
  {/if}
{/snippet}

{#snippet desktopOption(
  candidate: ExecutionTargetSelection | null,
  optionLabel: string,
  description: string,
  icon: "cpu" | "auto" = "cpu",
)}
  <button
    type="button"
    class="chat-runtime-option"
    class:chat-runtime-option-active={sameSelection(candidate)}
    role="option"
    aria-selected={sameSelection(candidate)}
    onclick={() => choose(candidate)}
  >
    {#if icon === "auto"}
      <WandSparkles size={14} strokeWidth={1.9} class="shrink-0 opacity-75" />
    {:else}
      <Cpu size={14} strokeWidth={1.9} class="shrink-0 opacity-75" />
    {/if}
    <span class="min-w-0 flex-1 text-left">
      <span class="block truncate text-[13px] font-medium text-surface-100">{optionLabel}</span>
      <span class="workshop-faint mt-0.5 block text-[11px]">{description}</span>
    </span>
    {@render checkmark(sameSelection(candidate))}
  </button>
{/snippet}

{#snippet mobileOption(
  candidate: ExecutionTargetSelection | null,
  optionLabel: string,
  description: string,
  index: number,
  icon: "cpu" | "auto" = "cpu",
)}
  <button
    type="button"
    class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
    aria-pressed={sameSelection(candidate)}
    onclick={() => choose(candidate)}
  >
    {#if icon === "auto"}
      <WandSparkles size={16} strokeWidth={1.9} class="shrink-0 text-primary-300" />
    {:else}
      <Cpu size={16} strokeWidth={1.9} class="shrink-0 text-primary-300" />
    {/if}
    <span class="mobile-turn-sheet-row-copy">
      <span class="mobile-turn-sheet-row-title">{optionLabel}</span>
      <span class="mobile-turn-sheet-row-subtitle">{description}</span>
    </span>
    {@render checkmark(sameSelection(candidate))}
  </button>
{/snippet}

{#if visible}
  <div class="chat-runtime-picker min-w-0">
    <button
      bind:this={triggerEl}
      type="button"
      class="chat-runtime-trigger worker-target-trigger"
      class:chat-runtime-trigger-open={open}
      class:worker-target-unavailable={unavailable}
      {disabled}
      aria-haspopup={layout.isMobile ? "dialog" : "listbox"}
      aria-expanded={open}
      aria-label="Worker workshop — {label}"
      title="Where Medousa runs delegated work"
      onclick={toggle}
    >
      <Cpu size={13} strokeWidth={1.9} class="shrink-0 opacity-75" />
      <span class="chat-runtime-trigger-label truncate">{label}</span>
      <ChevronDown size={12} strokeWidth={2} class="chat-runtime-trigger-chevron shrink-0" />
    </button>

    {#if open}
      <BodyPortal>
        {#if layout.isMobile}
          <div
            class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
            role="presentation"
            onclick={(event) => {
              if (event.target === event.currentTarget) close();
            }}
          >
            <div
              bind:this={sheetEl}
              class="mobile-sheet mobile-turn-sheet worker-target-sheet"
              role="dialog"
              aria-modal="true"
              aria-label="Choose worker workshop"
            >
              <div bind:this={sheetHeaderEl}>
                <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
                <header class="mobile-turn-sheet-header">
                  <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
                  <h2 class="mobile-turn-sheet-title">Worker workshop</h2>
                  <button type="button" class="mobile-sheet-done" onclick={close}>Done</button>
                </header>
              </div>
              <div class="mobile-turn-sheet-body worker-target-sheet-body">
                {#if unavailable}
                  <p class="worker-target-notice" role="status">
                    That workshop is no longer available. Choose another target before sending.
                  </p>
                {/if}
                <p class="mobile-turn-sheet-section-label">Routing</p>
                <div class="mobile-turn-sheet-group">
                  {@render mobileOption(null, `Default · ${defaultLabel}`, "Follow the workshop's current worker destination", 0)}
                  {#if parentTarget}
                    {@render mobileOption({ kind: "same_as_parent" }, "This workshop", `Keep workers on ${parentTarget.label}`, 1)}
                  {/if}
                  {#if agentTargets.length > 0}
                    {@render mobileOption({ kind: "auto" }, "Auto", `Let Medousa choose among ${agentTargets.length} authorized ${agentTargets.length === 1 ? "workshop" : "workshops"}`, parentTarget ? 2 : 1, "auto")}
                  {/if}
                </div>

                <p class="mobile-turn-sheet-section-label mt-5">Pin a workshop</p>
                <div class="mobile-turn-sheet-group">
                  {#each userTargets as target, index (target.runtime_id)}
                    {@render mobileOption({ kind: "exact", runtime_id: target.runtime_id }, target.label, targetDescription(target), index)}
                  {/each}
                </div>
                {#if executionTargets.error}
                  <p class="worker-target-error" role="status">
                    Could not refresh workshops. Existing choices are shown.
                  </p>
                {/if}
              </div>
            </div>
          </div>
        {:else}
          <div
            bind:this={menuEl}
            class="composer-anchored-menu worker-target-menu"
            role="listbox"
            aria-label="Choose worker workshop"
          >
            <header class="composer-anchored-menu-header">
              <div class="min-w-0">
                <h2 class="text-sm font-semibold text-surface-50">Worker workshop</h2>
                <p class="workshop-faint mt-0.5 text-xs">Where delegated work runs</p>
              </div>
            </header>
            <div class="composer-anchored-menu-body space-y-0.5">
              {#if unavailable}
                <p class="worker-target-notice" role="status">
                  That workshop is unavailable. Choose another target before sending.
                </p>
              {/if}
              {@render desktopOption(null, `Default · ${defaultLabel}`, "Follow the workshop's current worker destination")}
              {#if parentTarget}
                {@render desktopOption({ kind: "same_as_parent" }, "This workshop", `Keep workers on ${parentTarget.label}`)}
              {/if}
              {#if agentTargets.length > 0}
                {@render desktopOption({ kind: "auto" }, "Auto", `Let Medousa choose among ${agentTargets.length} authorized ${agentTargets.length === 1 ? "workshop" : "workshops"}`, "auto")}
              {/if}
              <div class="my-1 border-t border-surface-500/25" role="separator"></div>
              {#each userTargets as target (target.runtime_id)}
                {@render desktopOption({ kind: "exact", runtime_id: target.runtime_id }, target.label, targetDescription(target))}
              {/each}
              {#if executionTargets.error}
                <p class="px-2 py-1 text-[11px] text-content-error" role="status">
                  Could not refresh workshops. Existing choices are shown.
                </p>
              {/if}
            </div>
          </div>
        {/if}
      </BodyPortal>
    {/if}
  </div>
{/if}

<style>
  .worker-target-trigger {
    max-width: 9.5rem;
  }

  .worker-target-unavailable {
    color: rgb(253 230 138 / 0.9);
  }

  .worker-target-menu {
    width: min(22rem, calc(100vw - 1rem));
  }

  .worker-target-sheet {
    max-height: min(72dvh, 38rem);
  }

  .worker-target-sheet-body {
    overflow-y: auto;
  }

  .worker-target-notice,
  .worker-target-error {
    margin: 0.25rem 0.5rem 0.65rem;
    border: 1px solid rgb(245 158 11 / 0.22);
    border-radius: 0.65rem;
    background: rgb(120 53 15 / 0.16);
    padding: 0.55rem 0.65rem;
    color: rgb(253 230 138 / 0.85);
    font-size: 0.7rem;
    line-height: 1.35;
  }

  .worker-target-error {
    margin-top: 0.75rem;
  }
</style>
