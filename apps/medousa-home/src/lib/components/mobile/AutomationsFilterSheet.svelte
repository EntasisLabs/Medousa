<script lang="ts">
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
      class="mobile-sheet"
      role="dialog"
      aria-label="Automations section"
    >
      <header
        bind:this={headerEl}
        class="mobile-sheet-header mobile-activity-sheet-header scripts-workbench-sheet-header"
      >
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex w-full items-start justify-between gap-2">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Automations</h2>
            <p class="workshop-faint mt-0.5 text-xs">Scripts, flows, schedules, history</p>
          </div>
          <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={dismiss}>
            Done
          </button>
        </div>
      </header>

      <div class="mobile-you-scroll min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
        <section>
          <h3 class="mobile-you-section-title">Section</h3>
          <ul class="mt-2 space-y-1">
            {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
              <li>
                <button
                  type="button"
                  class="mobile-notes-filter-row {section === tab.id
                    ? 'mobile-notes-filter-row-active'
                    : ''}"
                  aria-pressed={section === tab.id}
                  onclick={() => selectSection(tab.id)}
                >
                  <span class="min-w-0 flex-1 text-left text-sm font-medium text-surface-100"
                    >{tab.label}</span
                  >
                </button>
              </li>
            {/each}
          </ul>
        </section>
      </div>
    </div>
  </div>
{/if}
