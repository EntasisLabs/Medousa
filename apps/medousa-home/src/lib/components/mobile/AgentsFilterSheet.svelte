<script lang="ts">
  import { haptic } from "$lib/haptics";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import {
    SKILL_FILTER_CHIPS,
    type SkillFilterChip,
  } from "$lib/utils/skillCatalog";

  interface Props {
    open: boolean;
    skillFilter: SkillFilterChip;
    onClose: () => void;
    onFilter: (filter: SkillFilterChip) => void;
    onRefresh: () => void;
  }

  let { open, skillFilter, onClose, onFilter, onRefresh }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    onClose();
  }

  function selectFilter(next: SkillFilterChip) {
    haptic("light");
    onFilter(next);
    onClose();
  }

  function refresh() {
    haptic("light");
    onRefresh();
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
      aria-label="Filter agents"
    >
      <header
        bind:this={headerEl}
        class="mobile-sheet-stack-header mobile-activity-sheet-header"
      >
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row items-start">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Agents</h2>
            <p class="workshop-faint mt-0.5 text-xs">Filter and refresh</p>
          </div>
          <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={dismiss}>
            Done
          </button>
        </div>
      </header>

      <div class="mobile-sheet-scroll space-y-6">
        <section>
          <h3 class="mobile-you-section-title">Show</h3>
          <ul class="mt-2 space-y-1">
            {#each SKILL_FILTER_CHIPS as chip (chip.id)}
              <li>
                <button
                  type="button"
                  class="mobile-notes-filter-row {skillFilter === chip.id
                    ? 'mobile-notes-filter-row-active'
                    : ''}"
                  aria-pressed={skillFilter === chip.id}
                  onclick={() => selectFilter(chip.id)}
                >
                  <span class="min-w-0 flex-1 text-left text-sm font-medium text-surface-100"
                    >{chip.label}</span
                  >
                </button>
              </li>
            {/each}
          </ul>
        </section>

        <section>
          <h3 class="mobile-you-section-title">Catalog</h3>
          <button
            type="button"
            class="mobile-notes-filter-row mt-2"
            onclick={refresh}
          >
            <span class="min-w-0 flex-1 text-left text-sm font-medium text-surface-100"
              >Refresh agents</span
            >
          </button>
        </section>
      </div>
    </div>
  </div>
{/if}
