<script lang="ts">
  import { Check, Plus, SlidersHorizontal, Upload } from "@lucide/svelte";
  import { onMount } from "svelte";
  import SpecialistImportWizard from "$lib/components/skills/SpecialistImportWizard.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
  import { automationDraftForSpecialist } from "$lib/utils/specialistAutomation";
  import {
    SKILL_FILTER_CHIPS,
    filterSkills,
    groupSkills,
    type SkillFilterChip,
  } from "$lib/utils/skillCatalog";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

  let search = $state("");
  let skillFilter = $state<SkillFilterChip>("all");
  let importWizardOpen = $state(false);
  let createMenuOpen = $state(false);
  let filterOpen = $state(false);
  let createOpen = $state(false);
  let createName = $state("");
  let createDescription = $state("");
  let createBusy = $state(false);
  let createError = $state<string | null>(null);

  onMount(() => {
    void catalog.refresh();
  });

  const filtered = $derived(filterSkills(catalog.manuscripts, search, skillFilter));
  const groups = $derived(groupSkills(filtered));
  const filterActive = $derived(search.trim().length > 0 || skillFilter !== "all");

  function closeMenus() {
    createMenuOpen = false;
    filterOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function openAgent(entry: ManuscriptCatalogEntry) {
    lmeWorkspace.openManuscript(entry.id, entry.name);
  }

  function runInChat(manuscriptId: string) {
    chat.draft = `/skill ${manuscriptId}`;
    onOpenChat();
  }

  function scheduleAgent(entry: ManuscriptCatalogEntry) {
    automationDraft.openCreate(
      automationDraftForSpecialist(entry, catalog.manuscriptDetail),
    );
    lmeWorkspace.setExplorerMode("schedules");
    layout.navigateDesktop("library", { bump: true });
  }

  function openCreate() {
    closeMenus();
    createOpen = true;
    createName = "";
    createDescription = "";
    createError = null;
  }

  function closeCreate() {
    if (createBusy) return;
    createOpen = false;
    createError = null;
  }

  function clearFilters() {
    search = "";
    skillFilter = "all";
  }

  async function submitCreate() {
    const name = createName.trim();
    if (!name) {
      createError = "Name is required.";
      return;
    }
    createBusy = true;
    createError = null;
    try {
      const detail = await catalog.createManuscript({
        name,
        description: createDescription.trim() || undefined,
      });
      createOpen = false;
      lmeWorkspace.openManuscript(detail.id, detail.name);
    } catch (err) {
      createError = err instanceof Error ? err.message : String(err);
    } finally {
      createBusy = false;
    }
  }
</script>

<svelte:window onclick={closeMenus} />

<aside class="lme-agents-explorer flex h-full min-h-0 w-full flex-col" aria-label="Agents">
  {#if catalog.error}
    <p class="shrink-0 px-3 py-2 text-sm text-error-400">{catalog.error}</p>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if catalog.loading && catalog.manuscripts.length === 0}
      <p class="workshop-muted px-3 py-2 text-sm">Loading…</p>
    {:else if filtered.length === 0}
      {#if filterActive}
        <p class="workshop-muted px-3 py-4 text-xs">No agents match.</p>
      {:else}
        <p class="workshop-muted px-3 py-4 text-xs">No agents yet.</p>
      {/if}
    {:else}
      {#each groups as group (group.label)}
        <section class="mb-2">
          <p class="workshop-faint px-3 pb-1 pt-1 text-[10px]">
            {group.label}
            <span class="text-surface-600">· {group.entries.length}</span>
          </p>
          <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
            {#each group.entries as entry (entry.id)}
              {@const active =
                lmeWorkspace.activeTab?.kind === "manuscript" &&
                lmeWorkspace.activeTab.manuscriptId === entry.id}
              <li>
                <div
                  class="flex items-start gap-1 px-3 py-2 {active
                    ? 'workshop-list-row-active'
                    : 'hover:bg-surface-800/70'}"
                >
                  <button
                    type="button"
                    class="min-w-0 flex-1 text-left"
                    onclick={() => openAgent(entry)}
                  >
                    <span class="block truncate text-sm font-medium text-surface-100">
                      {entry.name}
                    </span>
                    {#if entry.description}
                      <span class="workshop-faint mt-0.5 block truncate text-[11px]">
                        {entry.description}
                      </span>
                    {/if}
                  </button>
                  <div class="flex shrink-0 flex-col items-end gap-0.5 pt-0.5">
                    {#if entry.has_scripts}
                      <button
                        type="button"
                        class="workshop-text-action text-[10px]"
                        onclick={() => runInChat(entry.id)}
                      >
                        Run
                      </button>
                    {/if}
                    <button
                      type="button"
                      class="workshop-text-action text-[10px] text-surface-500"
                      onclick={() => scheduleAgent(entry)}
                    >
                      Schedule
                    </button>
                  </div>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>

  <footer class="lme-side-rail-dock">
    <div class="min-w-0 flex-1">
      {#if filterActive}
        <span class="workshop-faint truncate text-[11px]">Filtered</span>
      {/if}
    </div>

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-haspopup="menu"
        aria-expanded={createMenuOpen}
        aria-label="New agent"
        title="New"
        onclick={(event) => {
          event.stopPropagation();
          filterOpen = false;
          createMenuOpen = !createMenuOpen;
        }}
      >
        <Plus size={16} strokeWidth={1.75} />
      </button>
      {#if createMenuOpen}
        <div
          class="absolute bottom-full right-0 z-30 mb-1 min-w-[11rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
          role="menu"
          tabindex="-1"
          onclick={(event) => event.stopPropagation()}
          onkeydown={handleMenuKeydown}
        >
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item"
            onclick={openCreate}
          >
            <Plus size={14} strokeWidth={2} />
            New agent
          </button>
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item"
            onclick={() => {
              closeMenus();
              importWizardOpen = true;
            }}
          >
            <Upload size={14} strokeWidth={2} />
            Import
          </button>
        </div>
      {/if}
    </div>

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn {filterActive ? 'vault-dock-icon-btn-active' : ''}"
        aria-haspopup="menu"
        aria-expanded={filterOpen}
        aria-label="Filter agents"
        title="Filter"
        onclick={(event) => {
          event.stopPropagation();
          createMenuOpen = false;
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
          <div class="px-2.5 pb-2">
            <input
              class="input w-full text-xs"
              type="search"
              placeholder="Search agents…"
              bind:value={search}
              onclick={(event) => event.stopPropagation()}
            />
          </div>

          <p class="px-3 pb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
            Show
          </p>
          {#each SKILL_FILTER_CHIPS as chip (chip.id)}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={skillFilter === chip.id}
              class="vault-menu-item w-full justify-between {skillFilter === chip.id
                ? 'text-primary-200'
                : ''}"
              onclick={() => (skillFilter = chip.id)}
            >
              <span>{chip.label}</span>
              {#if skillFilter === chip.id}
                <Check size={14} strokeWidth={2} class="text-primary-300" />
              {/if}
            </button>
          {/each}

          <div class="my-1 border-t border-surface-500/35"></div>
          <button
            type="button"
            role="menuitem"
            class="vault-menu-item text-surface-400"
            onclick={() => void catalog.refresh()}
          >
            Refresh
          </button>

          {#if filterActive}
            <button
              type="button"
              role="menuitem"
              class="vault-menu-item text-surface-400"
              onclick={clearFilters}
            >
              Clear filters
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </footer>
</aside>

{#if createOpen}
  <div
    class="fixed inset-0 z-[80] flex items-center justify-center bg-surface-950/55 p-4 backdrop-blur-sm"
    role="presentation"
    onclick={closeCreate}
  >
    <div
      class="card w-full max-w-md space-y-4 p-5 shadow-xl"
      role="dialog"
      aria-label="New agent"
      onclick={(event) => event.stopPropagation()}
    >
      <div>
        <h2 class="text-sm font-semibold text-surface-50">New agent</h2>
        <p class="mt-1 text-xs text-surface-400">Name and optional description.</p>
      </div>
      <label class="block text-xs">
        <span class="workshop-label">Name</span>
        <input
          class="input mt-1 w-full text-sm"
          bind:value={createName}
          placeholder="Morning brief"
          disabled={createBusy}
        />
      </label>
      <label class="block text-xs">
        <span class="workshop-label">Description (optional)</span>
        <textarea
          class="input mt-1 w-full resize-none text-sm"
          rows="3"
          bind:value={createDescription}
          placeholder="What they help with…"
          disabled={createBusy}
        ></textarea>
      </label>
      {#if createError}
        <p class="text-xs text-warning-400">{createError}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={createBusy}
          onclick={closeCreate}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={createBusy}
          onclick={() => void submitCreate()}
        >
          {createBusy ? "Creating…" : "Create"}
        </button>
      </div>
    </div>
  </div>
{/if}

<SpecialistImportWizard
  open={importWizardOpen}
  onClose={() => (importWizardOpen = false)}
  onImported={(ids) => {
    const first = ids[0];
    if (!first) return;
    const entry = catalog.manuscripts.find((row) => row.id === first);
    lmeWorkspace.openManuscript(first, entry?.name ?? first);
  }}
/>
