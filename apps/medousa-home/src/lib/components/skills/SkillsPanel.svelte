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
  import { registerMobileBackHandler } from "$lib/mobileNavigation";

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

  function closeMobileDetail() {
    selectedSkillId = null;
    catalog.clearManuscriptDetail();
  }

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (!mobileDetailOpen) return false;
      closeMobileDetail();
      return true;
    });
  });
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
                Packaged skills she can run — import, tune tools, schedule.
              {:else}
                External tools through MCP — what’s connected right now.
              {/if}
            </p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            {#if activeTab === "specialists"}
              <button
                type="button"
                class="btn btn-sm variant-ghost-surface"
                onclick={() => catalog.refresh()}
              >
                Refresh
              </button>
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
          {#if activeTab === "specialists"}
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="btn btn-sm variant-ghost-surface"
                onclick={() => catalog.refresh()}
              >
                Refresh
              </button>
              <button
                type="button"
                class="btn btn-sm variant-ghost-surface"
                onclick={() => (importWizardOpen = true)}
              >
                Import
              </button>
            </div>
          {/if}
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

    {#if activeTab === "specialists" && (catalog.manuscripts.length > 0 || search.trim() || skillFilter !== "all")}
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
      <div class="mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-4">
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
          {#if search.trim() || skillFilter !== "all"}
            <p class="workshop-muted py-6 text-sm">No specialists match your filters.</p>
          {:else}
            <div class="mx-auto flex max-w-md flex-col items-start py-10">
              <h2 class="text-sm font-semibold text-surface-50">No specialists yet</h2>
              <p class="workshop-faint mt-2 text-sm leading-relaxed">
                Import a SKILL.md folder from Cursor, Hermes, or OpenClaw. Then open one to set tool
                policy, schedule it, or run it in chat.
              </p>
              <button
                type="button"
                class="btn btn-sm variant-filled-primary mt-5"
                onclick={() => (importWizardOpen = true)}
              >
                Import specialists…
              </button>
            </div>
          {/if}
        {:else}
          {#each skillGroups as group (group.label)}
            <section class="mb-4">
              <h2 class="settings-subsection-heading sticky top-0 z-[1] bg-surface-900/95 py-1 backdrop-blur-sm">
                {group.label}
                <span class="workshop-faint font-normal"> · {group.entries.length}</span>
              </h2>
              <div class="settings-toggle-list mt-2">
                {#each group.entries as entry (entry.id)}
                  <div
                    class="settings-toggle-row settings-metric-row {selectedSkillId === entry.id
                      ? 'workshop-list-row-active'
                      : ''}"
                  >
                    <button
                      type="button"
                      class="min-w-0 flex-1 text-left"
                      onclick={() => selectSkill(entry)}
                    >
                      <span class="flex flex-wrap items-center gap-2">
                        <span class="truncate text-sm font-medium text-surface-100">
                          {entry.name}
                        </span>
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
                      </span>
                      {#if entry.description}
                        <span class="workshop-faint mt-0.5 block truncate text-xs">
                          {entry.description}
                        </span>
                      {/if}
                    </button>
                    {#if !mobile}
                      <div class="flex shrink-0 items-center gap-2">
                        {#if entry.has_scripts}
                          <button
                            type="button"
                            class="workshop-text-action text-xs"
                            onclick={() => runSkill(entry.id)}
                          >
                            Run
                          </button>
                        {/if}
                        <button
                          type="button"
                          class="workshop-text-action text-xs"
                          onclick={() => onScheduleSkill(entry)}
                        >
                          Schedule…
                        </button>
                        <button
                          type="button"
                          class="workshop-text-action text-xs"
                          onclick={() => void openConfigPath(entry.path)}
                        >
                          Open
                        </button>
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
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
          onclick={closeMobileDetail}
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
      {:else if catalog.manuscripts.length === 0}
        <div class="py-2">
          <p class="settings-subsection-heading">Details</p>
          <p class="settings-subsection-lead mb-0">
            After you import, pick a specialist here to tune tools and schedule.
          </p>
        </div>
      {:else}
        <div class="py-2">
          <p class="settings-subsection-heading">Details</p>
          <p class="settings-subsection-lead mb-0">
            Open a specialist to set tool policy, schedule it, or run it in chat.
          </p>
        </div>
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
