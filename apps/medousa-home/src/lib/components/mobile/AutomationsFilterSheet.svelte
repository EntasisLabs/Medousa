<script lang="ts">
  import { Check } from "@lucide/svelte";
  import { AUTOMATIONS_SECTIONS } from "$lib/automationsSections";
  import { haptic } from "$lib/haptics";
  import type { AutomationsSection } from "$lib/stores/automationsNav.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    section: AutomationsSection;
    onClose: () => void;
    onSection: (section: AutomationsSection) => void;
  }

  let { open, section, onClose, onSection }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    onClose();
  }

  function selectSection(next: AutomationsSection) {
    haptic("light");
    onSection(next);
    onClose();
  }

  $effect(() => {
    if (!open || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
</script>

{#if open}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) dismiss();
    }}
  >
    <div
      bind:this={sheetEl}
      class="mobile-sheet mobile-sheet-medium automations-sheet"
      role="dialog"
      aria-label="Automations section"
    >
      <header
        bind:this={headerEl}
        class="mobile-sheet-stack-header mobile-activity-sheet-header"
      >
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row items-start">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Automations</h2>
            <p class="workshop-faint mt-0.5 text-xs">Scripts, flows, schedules, history</p>
          </div>
          <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={dismiss}>
            Done
          </button>
        </div>
      </header>

      <div class="mobile-sheet-scroll">
        <div class="mobile-turn-sheet-group" role="listbox" aria-label="Automations section">
          {#each AUTOMATIONS_SECTIONS as tab, index (tab.id)}
            <button
              type="button"
              class="mobile-turn-sheet-row {index > 0 ? 'mobile-turn-sheet-row-divider' : ''}"
              role="option"
              aria-selected={section === tab.id}
              onclick={() => selectSection(tab.id)}
            >
              <span class="mobile-turn-sheet-row-copy">
                <span class="mobile-turn-sheet-row-title">{tab.label}</span>
              </span>
              {#if section === tab.id}
                <Check size={18} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}
