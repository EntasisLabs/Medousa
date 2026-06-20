<script lang="ts">
  import { openConfigPath } from "$lib/config";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
  import {
    SKILL_FILTER_CHIPS,
    filterSkills,
    groupSkills,
    type SkillFilterChip,
  } from "$lib/utils/skillCatalog";
  import McpServersPanel from "$lib/components/skills/McpServersPanel.svelte";
  import SpecialistDetailEditor from "$lib/components/skills/SpecialistDetailEditor.svelte";
  import SpecialistImportWizard from "$lib/components/skills/SpecialistImportWizard.svelte";

  type CatalogTab = "specialists" | "connections";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onScheduleSkill: (entry: ManuscriptCatalogEntry) => void;
    onUseInAutomation: (entry: ManuscriptCatalogEntry) => void;
    mobile?: boolean;
    embedded?: boolean;
  }

  let {
    visible,
    onOpenChat,
    onScheduleSkill,
    onUseInAutomation,
    mobile = false,
    embedded = false,
  }: Props = $props();

  let activeTab = $state<CatalogTab>("specialists");
  let importWizardOpen = $state(false);
  let search = $state("");
  let skillFilter = $state<SkillFilterChip>("all");
  let selectedSkillId = $state<string | null>(null);

  const mobileDetailOpen = $derived(mobile && selectedSkillId !== null);

  $effect(() => {
    if (visible) {
      void catalog.refresh();
    }
  });

  const filteredSkills = $derived(
    filterSkills(catalog.manuscripts, search, skillFilter),
  );
  const skillGroups = $derived(groupSkills(filteredSkills));

  const selectedSkill = $derived(
    selectedSkillId
      ? (catalog.manuscripts.find((entry) => entry.id === selectedSkillId) ??
        null)
      : null,
  );

  function runSkill(manuscriptId: string) {
    chat.draft = `/skill ${manuscriptId}`;
    onOpenChat();
  }

  function selectSkill(entry: ManuscriptCatalogEntry) {
    selectedSkillId = entry.id;
    catalog.clearCapabilityDetail();
    void catalog.loadManuscriptDetail(entry.id);
  }

  function setTab(tab: CatalogTab) {
    activeTab = tab;
    search = "";
    selectedSkillId = null;
    catalog.clearCapabilityDetail();
    catalog.clearManuscriptDetail();
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 class="text-base font-semibold text-surface-50">Capabilities</h1>
            <p class="workshop-header-line mt-1">
              {#if activeTab === "specialists"}
                Specialists · runtime tool policy · {filteredSkills.length} configured
              {:else}
                MCP servers and capability connections
              {/if}
            </p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => catalog.refresh()}
            >
              Refresh
            </button>
            {#if activeTab === "specialists"}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                onclick={() => (importWizardOpen = true)}
              >
                Import…
              </button>
            {/if}
          </div>
        </div>
      {:else}
        <div class="flex items-center justify-between gap-2">
          <p class="workshop-faint text-xs">
            {#if activeTab === "specialists"}
              {filteredSkills.length} specialist{filteredSkills.length === 1 ? "" : "s"}
            {:else}
              Connections
            {/if}
          </p>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => catalog.refresh()}
            >
              Refresh
            </button>
            {#if activeTab === "specialists"}
              <button
                type="button"
                class="btn btn-sm variant-ghost-surface"
                onclick={() => (importWizardOpen = true)}
              >
                Import
              </button>
            {/if}
          </div>
        </div>
      {/if}

    <div class="workshop-tabs mt-3">
      <button
        type="button"
        class="workshop-tab {activeTab === 'specialists' ? 'workshop-tab-active' : ''}"
        onclick={() => setTab("specialists")}
      >
        Specialists
      </button>
      <button
        type="button"
        class="workshop-tab {activeTab === 'connections' ? 'workshop-tab-active' : ''}"
        onclick={() => setTab("connections")}
      >
        Connections
      </button>
    </div>

    {#if activeTab === "specialists"}
    <label class="mt-3 block">
      <span class="sr-only">Search specialists</span>
      <input
        class="input w-full max-w-md text-sm"
        type="search"
        placeholder="Search specialists…"
        bind:value={search}
      />
    </label>
      <div class="mt-2 flex flex-wrap gap-1.5">
        {#each SKILL_FILTER_CHIPS as chip (chip.id)}
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[11px] transition {skillFilter === chip.id
              ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
              : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
            onclick={() => (skillFilter = chip.id)}
          >
            {chip.label}
          </button>
        {/each}
      </div>
    {/if}
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if activeTab === "connections"}
      <div class="mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3">
        <p class="workshop-faint mb-3 text-xs">
          MCP servers and external tools available to Medousa at runtime.
        </p>
        <McpServersPanel />
      </div>
    {:else}
    <div
      class="workshop-list-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3 {mobileDetailOpen
        ? 'hidden'
        : ''}"
    >
      {#if catalog.loading && catalog.manuscripts.length === 0 && catalog.capabilities.length === 0}
        <p class="workshop-muted">Loading catalog…</p>
      {:else if catalog.error}
        <p class="text-sm text-error-400">{catalog.error}</p>
      {:else if activeTab === "specialists"}
        {#if filteredSkills.length === 0}
          <p class="workshop-muted">
            {search.trim() || skillFilter !== "all"
              ? "No specialists match your filters."
              : "No specialists yet. Use Import to bring SKILL.md folders from Cursor, Hermes, or OpenClaw."}
          </p>
        {:else}
          {#each skillGroups as group (group.label)}
            <section class="mb-4">
              <h2 class="workshop-section-title sticky top-0 bg-surface-900/95 py-1 backdrop-blur-sm">
                {group.label} · {group.entries.length}
              </h2>
              <ul class="mt-1 divide-y divide-surface-500/35 border-y border-surface-500/35">
                {#each group.entries as entry (entry.id)}
                  <li>
                    <div
                      class="flex items-center gap-3 px-2 py-2 transition hover:bg-surface-800/70 {selectedSkillId ===
                      entry.id
                        ? 'workshop-list-row-active'
                        : ''}"
                    >
                      <button
                        type="button"
                        class="min-w-0 flex-1 text-left"
                        onclick={() => selectSkill(entry)}
                      >
                        <div class="flex flex-wrap items-center gap-2">
                          <p class="truncate font-medium text-surface-100">
                            {entry.name}
                          </p>
                          {#if entry.openshell_enabled}
                            <span class="text-[10px] uppercase tracking-wide text-surface-500">
                              sandbox
                            </span>
                          {/if}
                          {#if entry.has_scripts}
                            <span class="text-[10px] uppercase tracking-wide text-surface-500">
                              scripts
                            </span>
                          {/if}
                        </div>
                        {#if entry.description}
                          <p class="workshop-faint mt-0.5 truncate text-[11px]">
                            {entry.description}
                          </p>
                        {/if}
                      </button>
                      {#if !mobile}
                        <div class="flex shrink-0 items-center gap-2">
                          {#if entry.has_scripts}
                            <button
                              type="button"
                              class="workshop-text-action"
                              onclick={() => runSkill(entry.id)}
                            >
                              Run
                            </button>
                          {/if}
                          <button
                            type="button"
                            class="workshop-text-action"
                            onclick={() => onScheduleSkill(entry)}
                          >
                            Schedule…
                          </button>
                          <button
                            type="button"
                            class="workshop-text-action"
                            onclick={() => void openConfigPath(entry.path)}
                          >
                            Open
                          </button>
                        </div>
                      {/if}
                    </div>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        {/if}
      {/if}
    </div>

    <aside
      class="{mobile
        ? mobileDetailOpen
          ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-y-auto'
          : 'hidden'
        : 'workshop-detail-pane w-[min(360px,40%)] shrink-0 overflow-y-auto border-l border-surface-500/40'} px-4 py-4"
    >
      {#if mobileDetailOpen}
        <button
          type="button"
          class="workshop-text-action mb-3 shrink-0 text-sm"
          onclick={() => {
            selectedSkillId = null;
            catalog.clearManuscriptDetail();
          }}
        >
          ← Back to list
        </button>
      {/if}
      {#if activeTab === "specialists" && selectedSkill}
        <SpecialistDetailEditor
          entry={selectedSkill}
          onRunSkill={runSkill}
          onUseInAutomation={onUseInAutomation}
          onScheduleSkill={onScheduleSkill}
          onOpenFile={(path) => void openConfigPath(path)}
        />
      {:else}
        <p class="workshop-muted text-sm">
          Select a specialist to inspect tool policy, schedule, or run in chat.
        </p>
      {/if}
    </aside>
    {/if}
  </div>

  <SpecialistImportWizard
    open={importWizardOpen}
    onClose={() => (importWizardOpen = false)}
    onImported={(ids) => {
      if (ids[0]) {
        selectedSkillId = ids[0];
        void catalog.loadManuscriptDetail(ids[0]);
      }
    }}
  />
</section>
