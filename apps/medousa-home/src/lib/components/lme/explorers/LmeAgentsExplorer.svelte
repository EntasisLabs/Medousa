<script lang="ts">
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

<aside class="lme-agents-explorer flex h-full min-h-0 w-full flex-col" aria-label="Agents">
  <div class="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-surface-500/45 px-3 py-2">
    <button type="button" class="btn btn-sm variant-filled-primary" onclick={openCreate}>
      New agent
    </button>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      onclick={() => (importWizardOpen = true)}
    >
      Import
    </button>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      onclick={() => void catalog.refresh()}
    >
      Refresh
    </button>
  </div>

  {#if catalog.manuscripts.length > 0 || search.trim() || skillFilter !== "all"}
    <div class="shrink-0 space-y-2 border-b border-surface-500/35 px-3 py-2">
      <input
        class="w-full rounded-lg border border-surface-500/45 bg-surface-900/70 px-2.5 py-1.5 text-sm text-surface-100 outline-none ring-primary-500/30 focus:ring-2"
        type="search"
        placeholder="Search agents…"
        bind:value={search}
      />
      <div class="flex flex-wrap gap-1">
        {#each SKILL_FILTER_CHIPS as chip (chip.id)}
          <button
            type="button"
            class="rounded-md px-2 py-0.5 text-[10px] transition {skillFilter === chip.id
              ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
              : 'text-surface-500 hover:bg-surface-800 hover:text-surface-300'}"
            onclick={() => (skillFilter = chip.id)}
          >
            {chip.label}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if catalog.error}
    <p class="mx-3 mt-3 rounded-container-token border border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-error-300">
      {catalog.error}
    </p>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto px-2 py-2">
    {#if catalog.loading && catalog.manuscripts.length === 0}
      <p class="px-1 py-3 text-sm text-surface-500">Loading agents…</p>
    {:else if filtered.length === 0}
      {#if search.trim() || skillFilter !== "all"}
        <p class="px-1 py-4 text-sm text-surface-500">No agents match your filters.</p>
      {:else}
        <div class="px-2 py-8">
          <h2 class="text-sm font-semibold text-surface-100">Name a helper</h2>
          <p class="mt-2 text-xs leading-relaxed text-surface-400">
            Start with a name — tune voice and schedule anytime. Or import a SKILL.md from Cursor,
            Hermes, or OpenClaw.
          </p>
          <div class="mt-4 flex flex-wrap gap-2">
            <button type="button" class="btn btn-sm variant-filled-primary" onclick={openCreate}>
              New agent
            </button>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => (importWizardOpen = true)}
            >
              Import
            </button>
          </div>
        </div>
      {/if}
    {:else}
      {#each groups as group (group.label)}
        <section class="mb-3">
          <h2 class="px-1 py-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
            {group.label}
            <span class="font-normal text-surface-600">· {group.entries.length}</span>
          </h2>
          <ul class="mt-0.5 space-y-0.5">
            {#each group.entries as entry (entry.id)}
              {@const active =
                lmeWorkspace.activeTab?.kind === "manuscript" &&
                lmeWorkspace.activeTab.manuscriptId === entry.id}
              <li>
                <div
                  class="flex items-start gap-1 rounded-md px-1.5 py-1.5 {active
                    ? 'bg-surface-700/80'
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
                      <span class="mt-0.5 block truncate text-[11px] text-surface-500">
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
                      class="workshop-text-action text-[10px]"
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
        <p class="mt-1 text-xs text-surface-400">
          Start with a name — tune voice and schedule anytime.
        </p>
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
        <span class="workshop-label">What they help with (optional)</span>
        <textarea
          class="input mt-1 w-full resize-none text-sm"
          rows="3"
          bind:value={createDescription}
          placeholder="A short job description…"
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
