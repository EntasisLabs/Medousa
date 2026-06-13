<script lang="ts">
  import { getMedousaConfigPaths, openConfigPath } from "$lib/config";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type {
    CapabilityBinding,
    CapabilityListEntry,
    ManuscriptCatalogEntry,
  } from "$lib/types/catalog";
  import {
    SKILL_FILTER_CHIPS,
    filterSkills,
    groupSkills,
    type SkillFilterChip,
  } from "$lib/utils/skillCatalog";
  import {
    TOOL_FILTER_CHIPS,
    bindingSourcesLabel,
    filterTools,
    groupTools,
    primaryEffectClass,
    type ToolFilterChip,
  } from "$lib/utils/toolCatalog";
  import McpServersPanel from "$lib/components/skills/McpServersPanel.svelte";
  import {
    isBindingDisabled,
    loadCapabilitiesOverlay,
    toggleCapabilityBinding,
    type DisabledBindingRef,
  } from "$lib/utils/capabilitiesApi";

  type CatalogTab = "skills" | "tools" | "services";

  let disabledBindings = $state<DisabledBindingRef[]>([]);
  let bindingBusy = $state<string | null>(null);
  let bindingMessage = $state<string | null>(null);

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onScheduleSkill: (entry: ManuscriptCatalogEntry) => void;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, onOpenChat, onScheduleSkill, mobile = false, embedded = false }: Props =
    $props();

  let activeTab = $state<CatalogTab>("skills");
  let search = $state("");
  let skillFilter = $state<SkillFilterChip>("all");
  let toolFilter = $state<ToolFilterChip>("all");
  let selectedSkillId = $state<string | null>(null);
  let selectedToolId = $state<string | null>(null);

  const mobileDetailOpen = $derived(
    mobile && (selectedSkillId !== null || selectedToolId !== null),
  );

  $effect(() => {
    if (visible) {
      void catalog.refresh();
      void refreshDisabledBindings();
    }
  });

  async function refreshDisabledBindings() {
    try {
      const overlay = await loadCapabilitiesOverlay();
      disabledBindings = overlay.disabledBindings;
    } catch {
      disabledBindings = [];
    }
  }

  const filteredSkills = $derived(
    filterSkills(catalog.manuscripts, search, skillFilter),
  );
  const skillGroups = $derived(groupSkills(filteredSkills));

  const filteredTools = $derived(
    filterTools(catalog.capabilities, search, toolFilter),
  );
  const toolGroups = $derived(groupTools(filteredTools));

  const selectedSkill = $derived(
    selectedSkillId
      ? (catalog.manuscripts.find((entry) => entry.id === selectedSkillId) ??
        null)
      : null,
  );

  const selectedTool = $derived(
    selectedToolId
      ? (catalog.capabilities.find((entry) => entry.id === selectedToolId) ??
        null)
      : null,
  );

  function runSkill(manuscriptId: string) {
    chat.draft = `/skill ${manuscriptId}`;
    onOpenChat();
  }

  function selectSkill(entry: ManuscriptCatalogEntry) {
    selectedSkillId = entry.id;
    selectedToolId = null;
  }

  function selectTool(entry: CapabilityListEntry) {
    selectedToolId = entry.id;
    selectedSkillId = null;
    bindingMessage = null;
    void catalog.loadCapabilityDetail(entry.id);
  }

  async function toggleBinding(
    capabilityId: string,
    source: string,
    reference: string,
    enabled: boolean,
  ) {
    const key = `${capabilityId}:${source}:${reference}`;
    bindingBusy = key;
    bindingMessage = null;
    try {
      const result = await toggleCapabilityBinding(
        capabilityId,
        source,
        reference,
        enabled,
      );
      bindingMessage = result.message;
      await refreshDisabledBindings();
      await catalog.refresh();
      if (selectedToolId) {
        await catalog.loadCapabilityDetail(selectedToolId);
      }
    } catch (err) {
      bindingMessage = err instanceof Error ? err.message : String(err);
    } finally {
      bindingBusy = null;
    }
  }

  function setTab(tab: CatalogTab) {
    activeTab = tab;
    search = "";
    selectedSkillId = null;
    selectedToolId = null;
    catalog.clearCapabilityDetail();
  }

  async function openCapabilitiesFile() {
    const paths = await getMedousaConfigPaths();
    await openConfigPath(paths.capabilities);
  }

  function allBindings(
    detail: NonNullable<typeof catalog.capabilityDetail>,
  ): CapabilityBinding[] {
    return [
      ...detail.implementations.grapheme,
      ...detail.implementations.mcp,
    ].sort((left, right) => left.priority - right.priority);
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 class="text-base font-semibold text-surface-50">Skills &amp; Tools</h1>
            <p class="workshop-header-line mt-1">
              {#if activeTab === "skills"}
                Manuscripts she can run · {filteredSkills.length} skill{filteredSkills.length === 1 ? "" : "s"}
              {:else if activeTab === "tools"}
                Tools she can reach · {filteredTools.length} tool{filteredTools.length === 1 ? "" : "s"}
              {:else}
                MCP servers Medousa can call
              {/if}
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => catalog.refresh()}
          >
            Refresh
          </button>
        </div>
      {:else}
        <div class="flex items-center justify-between gap-2">
          <p class="workshop-faint text-xs">
            {#if activeTab === "skills"}
              {filteredSkills.length} skill{filteredSkills.length === 1 ? "" : "s"}
            {:else if activeTab === "tools"}
              {filteredTools.length} tool{filteredTools.length === 1 ? "" : "s"}
            {:else}
              MCP services
            {/if}
          </p>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => catalog.refresh()}
          >
            Refresh
          </button>
        </div>
      {/if}

    <div class="workshop-tabs mt-3">
      <button
        type="button"
        class="workshop-tab {activeTab === 'skills' ? 'workshop-tab-active' : ''}"
        onclick={() => setTab("skills")}
      >
        Skills
      </button>
      <button
        type="button"
        class="workshop-tab {activeTab === 'tools' ? 'workshop-tab-active' : ''}"
        onclick={() => setTab("tools")}
      >
        Tools
      </button>
      <button
        type="button"
        class="workshop-tab {activeTab === 'services' ? 'workshop-tab-active' : ''}"
        onclick={() => setTab("services")}
      >
        Services
      </button>
    </div>

    {#if activeTab !== "services"}
    <label class="mt-3 block">
      <span class="sr-only">
        Search {activeTab === "skills" ? "skills" : "tools"}
      </span>
      <input
        class="input w-full max-w-md text-sm"
        type="search"
        placeholder={activeTab === "skills"
          ? "Search skills…"
          : "Search tools…"}
        bind:value={search}
      />
    </label>
    {/if}

    {#if activeTab === "skills"}
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
    {:else}
      <div class="mt-2 flex flex-wrap gap-1.5">
        {#each TOOL_FILTER_CHIPS as chip (chip.id)}
          <button
            type="button"
            class="rounded-md px-2 py-1 text-[11px] transition {toolFilter === chip.id
              ? 'bg-surface-700 text-primary-300 ring-1 ring-inset ring-primary-500/35'
              : 'text-surface-400 hover:bg-surface-800 hover:text-surface-200'}"
            onclick={() => (toolFilter = chip.id)}
          >
            {chip.label}
          </button>
        {/each}
      </div>
    {/if}
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    {#if activeTab === "services"}
      <div class="mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3">
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
      {:else if activeTab === "skills"}
        {#if filteredSkills.length === 0}
          <p class="workshop-muted">
            {search.trim() || skillFilter !== "all"
              ? "No skills match your filters."
              : "No skills yet. Import with medousa skill-import."}
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
      {:else if filteredTools.length === 0}
        <p class="workshop-muted">
          {search.trim() || toolFilter !== "all"
            ? "No tools match your filters."
            : "No tools registered yet."}
        </p>
      {:else}
        {#each toolGroups as group (group.label)}
          <section class="mb-4">
            <h2 class="workshop-section-title sticky top-0 bg-surface-900/95 py-1 backdrop-blur-sm">
              {group.label} · {group.entries.length}
            </h2>
            <ul class="mt-1 divide-y divide-surface-500/35 border-y border-surface-500/35">
              {#each group.entries as entry (entry.id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedToolId ===
                    entry.id
                      ? 'workshop-list-row-active'
                      : ''}"
                    onclick={() => selectTool(entry)}
                  >
                    <div class="min-w-0 flex-1">
                      <div class="flex flex-wrap items-center gap-2">
                        <p class="truncate font-medium text-surface-100">
                          {entry.title}
                        </p>
                        {#if entry.has_grapheme || entry.bindings_summary?.some((binding) => binding.source === "grapheme")}
                          <span class="text-[10px] uppercase tracking-wide text-surface-500">
                            grapheme
                          </span>
                        {/if}
                        {#if entry.has_mcp || entry.bindings_summary?.some((binding) => binding.source === "mcp")}
                          <span class="text-[10px] uppercase tracking-wide text-surface-500">
                            mcp
                          </span>
                        {/if}
                        {#if primaryEffectClass(entry)}
                          <span class="text-[10px] uppercase tracking-wide text-warning-400/80">
                            {primaryEffectClass(entry)}
                          </span>
                        {/if}
                      </div>
                      {#if entry.description}
                        <p class="workshop-faint mt-0.5 truncate text-[11px]">
                          {entry.description}
                        </p>
                      {/if}
                      <p class="workshop-faint mt-0.5 truncate font-mono text-[11px]">
                        {entry.id} · {bindingSourcesLabel(entry)}
                      </p>
                    </div>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
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
            selectedToolId = null;
            catalog.clearCapabilityDetail();
          }}
        >
          ← Back to list
        </button>
      {/if}
      {#if activeTab === "skills" && selectedSkill}
        <h2 class="workshop-section-title">Skill detail</h2>
        <p class="mt-2 font-medium text-surface-100">{selectedSkill.name}</p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selectedSkill.id}</p>

        {#if selectedSkill.description}
          <p class="mt-3 text-sm leading-relaxed text-surface-300">
            {selectedSkill.description}
          </p>
        {/if}

        <dl class="mt-4 space-y-2 text-xs">
          <div>
            <dt class="workshop-label">Scope</dt>
            <dd class="mt-0.5 text-surface-200">{selectedSkill.scope}</dd>
          </div>
          <div>
            <dt class="workshop-label">Path</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">
              {selectedSkill.path}
            </dd>
          </div>
          {#if selectedSkill.openshell_enabled}
            <div>
              <dt class="workshop-label">Sandbox</dt>
              <dd class="mt-0.5 text-surface-200">OpenShell enabled</dd>
            </div>
          {/if}
          {#if selectedSkill.scripts.length > 0}
            <div>
              <dt class="workshop-label">Scripts</dt>
              <dd class="mt-0.5 space-y-1 text-surface-300">
                {#each selectedSkill.scripts as script (script.relative_path)}
                  <p class="font-mono text-[11px]">
                    {script.relative_path}
                    <span class="text-surface-500">({script.risk_class})</span>
                  </p>
                {/each}
              </dd>
            </div>
          {/if}
        </dl>

        <div class="mt-5 flex flex-wrap gap-3">
          {#if selectedSkill.has_scripts}
            <button
              type="button"
              class="workshop-text-action"
              onclick={() => runSkill(selectedSkill.id)}
            >
              Run in chat
            </button>
          {/if}
          <button
            type="button"
            class="workshop-text-action"
            onclick={() => onScheduleSkill(selectedSkill)}
          >
            Schedule…
          </button>
          <button
            type="button"
            class="workshop-text-action"
            onclick={() => void openConfigPath(selectedSkill.path)}
          >
            Open file
          </button>
        </div>
      {:else if activeTab === "tools" && selectedTool}
        <h2 class="workshop-section-title">Tool detail</h2>
        <p class="mt-2 font-medium text-surface-100">{selectedTool.title}</p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selectedTool.id}</p>

        {#if selectedTool.description}
          <p class="mt-3 text-sm leading-relaxed text-surface-300">
            {selectedTool.description}
          </p>
        {/if}

        {#if catalog.capabilityDetailLoading}
          <p class="workshop-muted mt-4 text-xs">Loading bindings…</p>
        {:else if catalog.capabilityDetailError}
          <p class="mt-4 text-xs text-warning-400">{catalog.capabilityDetailError}</p>
        {:else if catalog.capabilityDetail}
          {#if catalog.capabilityDetail.recommended}
            <dl class="mt-4 space-y-2 text-xs">
              <div>
                <dt class="workshop-label">Recommended</dt>
                <dd class="mt-0.5 font-mono text-surface-200">
                  {catalog.capabilityDetail.recommended.source} ·
                  {catalog.capabilityDetail.recommended.reference}
                </dd>
              </div>
            </dl>
          {/if}

          <div class="mt-4">
            <h3 class="workshop-label">Bindings</h3>
            <ul class="mt-2 space-y-2">
              {#each allBindings(catalog.capabilityDetail) as binding (binding.reference + binding.source)}
                <li class="rounded-md border border-surface-500/35 px-3 py-2 text-xs">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="font-medium uppercase text-surface-200">
                      {binding.source}
                    </span>
                    <span
                      class="text-[10px] uppercase tracking-wide {binding.available
                        ? 'text-primary-300'
                        : 'text-surface-500'}"
                    >
                      {binding.available ? "available" : "unavailable"}
                    </span>
                    {#if binding.effect_class}
                      <span class="text-[10px] uppercase tracking-wide text-warning-400/80">
                        {binding.effect_class}
                      </span>
                    {/if}
                    <label class="ml-auto inline-flex items-center gap-2 text-surface-300">
                      <input
                        type="checkbox"
                        class="checkbox"
                        checked={!isBindingDisabled(
                          disabledBindings,
                          selectedTool.id,
                          binding.source,
                          binding.reference,
                        )}
                        disabled={bindingBusy !== null}
                        onchange={(event) =>
                          void toggleBinding(
                            selectedTool.id,
                            binding.source,
                            binding.reference,
                            (event.currentTarget as HTMLInputElement).checked,
                          )}
                      />
                      Enabled
                    </label>
                  </div>
                  <p class="mt-1 font-mono text-[11px] text-surface-300">
                    {binding.reference}
                  </p>
                  {#if binding.invoke_via}
                    <p class="workshop-faint mt-1">
                      via {binding.invoke_via} · priority {binding.priority}
                    </p>
                  {/if}
                  {#if binding.unavailable_reason}
                    <p class="mt-1 text-warning-400/90">{binding.unavailable_reason}</p>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>

          {#if catalog.capabilityDetail.gateway_unreachable}
            <p class="workshop-faint mt-3 text-xs text-warning-400">
              MCP gateway unreachable — MCP bindings may show unavailable until sync.
            </p>
          {/if}

          {#if bindingMessage}
            <p class="mt-3 text-xs text-surface-300">{bindingMessage}</p>
          {/if}
        {:else}
          <dl class="mt-4 space-y-2 text-xs">
            <div>
              <dt class="workshop-label">Bindings</dt>
              <dd class="mt-0.5 text-surface-200">
                {bindingSourcesLabel(selectedTool)}
              </dd>
            </div>
          </dl>
        {/if}

        <button
          type="button"
          class="workshop-text-action mt-5"
          onclick={() => void openCapabilitiesFile()}
        >
          Open capabilities.toml
        </button>
      {:else}
        <p class="workshop-muted text-sm">
          {#if activeTab === "skills"}
            Select a skill to inspect scripts and schedule, or use row actions to
            run immediately.
          {:else}
            Select a tool to see binding summary.
          {/if}
        </p>
      {/if}
    </aside>
    {/if}
  </div>
</section>
