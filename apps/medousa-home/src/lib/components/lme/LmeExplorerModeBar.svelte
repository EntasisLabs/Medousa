<script lang="ts">
  import {
    LME_AUTOMATIONS_PRIMARY_MODES,
    LME_AUTOMATIONS_SECONDARY_MODES,
    LME_LIBRARY_MODES,
    automationsStripPageForMode,
    type LmeExplorerFamily,
    type LmeExplorerModeDef,
  } from "$lib/utils/lmeExplorerModes";
  import { lmeWorkspace, type LmeExplorerMode } from "$lib/stores/lmeWorkspace.svelte";
  import { ChevronLeft, ChevronRight } from "@lucide/svelte";

  interface Props {
    family?: LmeExplorerFamily;
  }

  let { family = "library" }: Props = $props();

  const active = $derived(lmeWorkspace.explorerMode);

  /** Manual page override while browsing; null = follow active mode. */
  let automationsPageOverride = $state<0 | 1 | null>(null);

  const automationsPage = $derived<0 | 1>(
    family === "automations"
      ? (automationsPageOverride ?? automationsStripPageForMode(active))
      : 0,
  );

  $effect(() => {
    if (family !== "automations") {
      if (automationsPageOverride !== null) automationsPageOverride = null;
      return;
    }
    const modePage = automationsStripPageForMode(active);
    if (automationsPageOverride !== null && modePage === automationsPageOverride) {
      automationsPageOverride = null;
    }
  });

  function select(modeId: LmeExplorerMode) {
    lmeWorkspace.setExplorerMode(modeId);
  }

  function goAutomationsPage(page: 0 | 1) {
    automationsPageOverride = page;
  }

  function modeBtnClass(modeId: LmeExplorerMode): string {
    const pad =
      family === "automations"
        ? "gap-1 px-1.5"
        : "gap-1.5 px-2";
    return `lme-side-mode-btn inline-flex h-8 shrink-0 items-center rounded-md ${pad} text-[0.72rem] font-medium tracking-tight transition-colors ${
      active === modeId
        ? "bg-surface-700/90 text-surface-50"
        : "text-surface-400 hover:bg-surface-800/80 hover:text-surface-200"
    }`;
  }
</script>

{#snippet modeTab(entry: LmeExplorerModeDef)}
  {@const Icon = entry.icon}
  <button
    type="button"
    role="tab"
    aria-selected={active === entry.id}
    class={modeBtnClass(entry.id)}
    title={entry.label}
    aria-label={entry.label}
    onclick={() => select(entry.id)}
  >
    <Icon size={14} strokeWidth={1.75} />
    <span class="lme-side-mode-label whitespace-nowrap">{entry.label}</span>
  </button>
{/snippet}

{#snippet pageChevron(direction: "prev" | "next")}
  <button
    type="button"
    class="lme-side-mode-btn inline-flex size-8 shrink-0 items-center justify-center rounded-md text-surface-400 transition-colors hover:bg-surface-800/80 hover:text-surface-200"
    title={direction === "next" ? "More automations" : "Back"}
    aria-label={direction === "next"
      ? "Show history"
      : "Show scripts, agents, flows, and schedules"}
    onclick={() => goAutomationsPage(direction === "next" ? 1 : 0)}
  >
    {#if direction === "next"}
      <ChevronRight size={15} strokeWidth={2} />
    {:else}
      <ChevronLeft size={15} strokeWidth={2} />
    {/if}
  </button>
{/snippet}

<div
  class="lme-side-mode-bar lme-explorer-mode-bar flex shrink-0 items-center gap-0.5 border-b border-surface-500/25 px-1.5 py-1"
  role="tablist"
  aria-label={family === "automations" ? "Automations modes" : "Library modes"}
  data-debug-label="lme-explorer-mode-bar"
  data-family={family}
>
  {#if family === "library"}
    <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
      {#each LME_LIBRARY_MODES as entry (entry.id)}
        {@render modeTab(entry)}
      {/each}
    </div>
  {:else}
    <div class="lme-automations-strip relative min-w-0 flex-1 overflow-hidden">
      <div
        class="lme-automations-strip-track flex w-[200%] transition-transform duration-220 ease-out"
        style="transform: translateX(-{automationsPage * 50}%)"
      >
        <div class="flex w-1/2 min-w-0 items-center gap-px overflow-x-auto pr-0.5">
          {#each LME_AUTOMATIONS_PRIMARY_MODES as entry (entry.id)}
            {@render modeTab(entry)}
          {/each}
          {@render pageChevron("next")}
        </div>
        <div class="flex w-1/2 min-w-0 items-center gap-0.5 pl-0.5">
          {@render pageChevron("prev")}
          {#each LME_AUTOMATIONS_SECONDARY_MODES as entry (entry.id)}
            {@render modeTab(entry)}
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .lme-automations-strip-track {
    will-change: transform;
  }

  :global(.duration-220) {
    transition-duration: 220ms;
  }
</style>
