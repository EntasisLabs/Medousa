<script lang="ts">
  import { Plus, SlidersHorizontal } from "@lucide/svelte";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import {
    GRAPHEME_STARTER_RECIPES,
    type GraphemeRecipe,
  } from "$lib/grapheme/graphemeRecipes";
  import { flows } from "$lib/stores/flows.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import type { WorkflowListEntry } from "$lib/types/workflow";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { onMount } from "svelte";

  let search = $state("");
  let filterOpen = $state(false);

  onMount(() => {
    void flows.refresh();
  });

  const filterActive = $derived(search.trim().length > 0);

  const filtered = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const rows = [...flows.workflows].sort(
      (left, right) =>
        new Date(right.created_at_utc).getTime() -
        new Date(left.created_at_utc).getTime(),
    );
    if (!query) return rows;
    return rows.filter((entry) => {
      const haystack = [
        flows.labelFor(entry),
        entry.workflow_id,
        entry.status,
        entry.strategy,
        entry.scheduled_recurring_id ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  });

  const filteredRecipes = $derived(
    GRAPHEME_STARTER_RECIPES.filter((recipe) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return (
        recipe.title.toLowerCase().includes(needle) ||
        recipe.subtitle.toLowerCase().includes(needle)
      );
    }),
  );

  function closeMenus() {
    filterOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function statusChipVariant(entry: WorkflowListEntry): "scheduled" | "paused" | "running" {
    if (entry.status === "failed") return "running";
    if (entry.status === "running" || entry.status === "enqueued") return "running";
    if (entry.status === "canceled") return "paused";
    return "scheduled";
  }

  function openFlow(entry: WorkflowListEntry) {
    lmeWorkspace.openFlow(entry.workflow_id, flows.labelFor(entry));
  }

  function startNew() {
    lmeWorkspace.openNewFlow();
  }

  function startFromRecipe(recipe: GraphemeRecipe) {
    flows.openComposerWithRecipe(recipe);
    lmeWorkspace.focusFlowComposerTab(recipe.flowName?.trim() || "New flow");
  }

  const draftActive = $derived(
    lmeWorkspace.activeTab?.kind === "flow" && lmeWorkspace.activeTab.workflowId === null,
  );
</script>

<svelte:window onclick={closeMenus} />

<aside class="lme-flows-explorer flex h-full min-h-0 w-full flex-col" aria-label="Flows">
  {#if flows.error}
    <p class="shrink-0 px-3 py-2 text-sm text-error-400">{flows.error}</p>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if flows.loading && flows.workflows.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else if filtered.length === 0}
      {#if filterActive}
        <p class="workshop-muted px-3 py-4 text-xs">No flows match.</p>
      {:else if settings.showWorkshopGuidance && filteredRecipes.length > 0}
        <p class="workshop-faint px-3 pb-1 pt-1 text-[10px]">Recipes</p>
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filteredRecipes as recipe (recipe.id)}
            <li>
              <button
                type="button"
                class="flex w-full flex-col px-3 py-2.5 text-left transition hover:bg-surface-800/70"
                onclick={() => startFromRecipe(recipe)}
              >
                <span class="text-sm font-medium text-surface-100">{recipe.title}</span>
                <span class="workshop-faint mt-0.5 text-[11px] leading-snug">
                  {recipe.subtitle}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="workshop-muted px-3 py-4 text-xs">No flows yet.</p>
      {/if}
    {:else}
      {#if draftActive || flows.composerOpen}
        <ul class="mb-2 divide-y divide-surface-500/35 border-y border-surface-500/35">
          <li>
            <button
              type="button"
              class="flex w-full flex-col px-3 py-2 text-left transition hover:bg-surface-800/70 {draftActive
                ? 'workshop-list-row-active'
                : ''}"
              onclick={() => lmeWorkspace.focusFlowComposerTab()}
            >
              <span class="truncate text-sm font-medium text-surface-100">
                {flows.composerDraft.name.trim() || "Untitled flow"}
              </span>
              <span class="workshop-faint mt-0.5 text-[11px]">Draft</span>
            </button>
          </li>
        </ul>
      {/if}

      <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
        {#each filtered as entry (entry.workflow_id)}
          {@const active =
            lmeWorkspace.activeTab?.kind === "flow" &&
            lmeWorkspace.activeTab.workflowId === entry.workflow_id}
          <li>
            <button
              type="button"
              class="flex w-full items-start gap-2 px-3 py-2 text-left transition hover:bg-surface-800/70 {active
                ? 'workshop-list-row-active'
                : ''}"
              onclick={() => openFlow(entry)}
            >
              <span class="min-w-0 flex-1">
                <span class="flex flex-wrap items-center gap-1.5">
                  <span class="truncate text-sm font-medium text-surface-100">
                    {flows.labelFor(entry)}
                  </span>
                  <WorkshopLivelinessChip variant={statusChipVariant(entry)} />
                </span>
                <span class="workshop-faint mt-0.5 block truncate text-[11px]">
                  {entry.step_count} steps · {entry.strategy}
                  {#if entry.scheduled_recurring_id}
                    · scheduled
                  {/if}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <footer class="lme-side-rail-dock" use:portLmeDock>
    <div class="min-w-0 flex-1">
      {#if filterActive}
        <span class="workshop-faint truncate text-[11px]">Filtered</span>
      {/if}
    </div>

    <button
      type="button"
      class="vault-dock-icon-btn"
      aria-label="New flow"
      title="New"
      onclick={startNew}
    >
      <Plus size={16} strokeWidth={1.75} />
    </button>

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn {filterActive ? 'vault-dock-icon-btn-active' : ''}"
        aria-haspopup="menu"
        aria-expanded={filterOpen}
        aria-label="Filter flows"
        title="Filter"
        onclick={(event) => {
          event.stopPropagation();
          filterOpen = !filterOpen;
        }}
      >
        <SlidersHorizontal size={15} strokeWidth={1.75} />
      </button>
      {#if filterOpen}
        <div
          class="vault-notes-filter-menu absolute bottom-full right-0 z-30 mb-1 w-[min(17.5rem,calc(100vw-2rem))] rounded-lg border border-surface-500/50 bg-surface-900 py-2 shadow-xl"
          role="menu"
          tabindex="-1"
          onclick={(event) => event.stopPropagation()}
          onkeydown={handleMenuKeydown}
        >
          <div class="px-2.5 pb-1">
            <input
              class="input w-full text-xs"
              type="search"
              placeholder="Search flows…"
              bind:value={search}
              onclick={(event) => event.stopPropagation()}
            />
          </div>
          <div class="my-1 border-t border-surface-500/35"></div>
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item text-surface-400"
            onclick={() => void flows.refresh()}
          >
            Refresh
          </button>
          {#if filterActive}
            <button
              type="button"
              role="menuitem"
              class="vault-menu-item text-surface-400"
              onclick={() => {
                search = "";
              }}
            >
              Clear
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </footer>
</aside>
