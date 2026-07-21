<script lang="ts">
  import { SlidersHorizontal } from "@lucide/svelte";
  import FlowsPanel from "$lib/components/automations/FlowsPanel.svelte";
  import HistoryPanel from "$lib/components/automations/HistoryPanel.svelte";
  import ScheduleCreatePopover from "$lib/components/automations/ScheduleCreatePopover.svelte";
  import ScheduleDetailEditor from "$lib/components/automations/ScheduleDetailEditor.svelte";
  import ScriptsWorkbenchPanel from "$lib/components/automations/ScriptsWorkbenchPanel.svelte";
  import { AUTOMATIONS_SECTIONS } from "$lib/automationsSections";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { automationsNav, type AutomationsSection } from "$lib/stores/automationsNav.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import type { RecurringDefinitionEntry } from "$lib/types/recurring";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    /** Hosted inside LME — section tabs owned by LME mode bar. */
    lmeHosted?: boolean;
    /** When set (LME), keep this section active. */
    forcedSection?: AutomationsSection | null;
  }

  let {
    visible,
    mobile = false,
    embedded = false,
    lmeHosted = false,
    forcedSection = null,
  }: Props = $props();

  let section = $state<AutomationsSection>("scripts");

  let search = $state("");
  let filterOpen = $state(false);
  let selectedId = $state<string | null>(null);
  const filterActive = $derived(search.trim().length > 0);

  /** LME: highlight from the open schedule tab; standalone: local selection. */
  const activeRecurringId = $derived.by(() => {
    if (lmeHosted) {
      const tab = lmeWorkspace.activeTab;
      return tab?.kind === "schedule" ? tab.recurringId : null;
    }
    return selectedId;
  });

  /** Standalone mobile only — create is a popover, not a detail takeover. */
  const mobileDetailOpen = $derived(mobile && !lmeHosted && selectedId !== null);

  const counts = $derived(automations.activeCount());

  const filtered = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const rows = [...automations.definitions].sort(
      (left, right) =>
        new Date(left.next_run_at_utc).getTime() -
        new Date(right.next_run_at_utc).getTime(),
    );
    if (!query) return rows;
    return rows.filter((entry) => {
      const haystack = [
        automations.labelFor(entry),
        entry.recurring_id,
        entry.cron_expr,
        entry.manuscript_id ?? "",
        entry.delivery_label ?? "",
        automations.originFor(entry),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  });

  const selected = $derived(
    activeRecurringId
      ? (automations.definitions.find(
          (entry) => entry.recurring_id === activeRecurringId,
        ) ?? null)
      : null,
  );

  $effect(() => {
    if (!visible) return;
    if (forcedSection) {
      if (section !== forcedSection) section = forcedSection;
      return;
    }
    const pending = automationsNav.consumeSection();
    if (pending && section !== pending) section = pending;
  });

  $effect(() => {
    if (!visible || !flowDraft.openComposer || flowDraft.pendingRefs.length === 0) return;
    void flows
      .applyFromSliceRefs(flowDraft.pendingRefs, flowDraft.seedDraft.name)
      .finally(() => {
        section = "flows";
        const title = flows.composerDraft.name.trim() || "New flow";
        flowDraft.clear();
        lmeWorkspace.focusFlowComposerTab(title);
      });
  });

  $effect(() => {
    if (!visible) return;
    if (section === "schedules") {
      void automations.refresh();
    }
  });

  $effect(() => {
    if (lmeHosted) return;
    const id = selected?.recurring_id;
    if (!visible || !id) return;
    void automations.loadRuns(id);
  });

  function selectEntry(entry: RecurringDefinitionEntry) {
    automationDraft.clearCreate();
    if (lmeHosted) {
      lmeWorkspace.openSchedule(entry.recurring_id, automations.labelFor(entry));
      return;
    }
    selectedId = entry.recurring_id;
  }

  function closeMobileDetail() {
    selectedId = null;
    automationDraft.clearCreate();
  }

  $effect(() => {
    if (!mobile || !visible || section !== "schedules") return;
    return registerMobileBackHandler(() => {
      if (!mobileDetailOpen) return false;
      closeMobileDetail();
      return true;
    });
  });
</script>

<section
  class="cron-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {mobile
    ? 'cron-panel-mobile'
    : ''} {visible ? '' : 'hidden'}"
>
  {#if section === "scripts"}
    {#if !lmeHosted}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      <div class="flex flex-wrap items-center justify-between gap-3">
        {#if !embedded}
          <div>
            <h1 class="text-base font-semibold text-surface-50">Automations</h1>
            <p class="workshop-header-line mt-1">Scripts workbench · write, run, add to flow</p>
          </div>
        {/if}
      </div>
      <div class="workshop-tabs workshop-tabs-mobile mt-3">
        {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {section === tab.id ? 'workshop-tab-active' : ''}"
            onclick={() => (section = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>
    </header>
    {/if}
    <ScriptsWorkbenchPanel visible={true} {mobile} {embedded} />
  {:else if section === "flows"}
    {#if !lmeHosted}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      <div class="flex flex-wrap items-center justify-between gap-3">
        {#if !embedded}
          <div>
            <h1 class="text-base font-semibold text-surface-50">Automations</h1>
            <p class="workshop-header-line mt-1">Scripts · flows · schedules · history</p>
          </div>
        {/if}
      </div>
      <div class="workshop-tabs workshop-tabs-mobile mt-3">
        {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {section === tab.id ? 'workshop-tab-active' : ''}"
            onclick={() => (section = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>
    </header>
    {/if}
    <FlowsPanel visible={true} {mobile} {embedded} />
  {:else if section === "history"}
    {#if !lmeHosted}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      <div class="flex flex-wrap items-center justify-between gap-3">
        {#if !embedded}
          <div>
            <h1 class="text-base font-semibold text-surface-50">Automations</h1>
            <p class="workshop-header-line mt-1">Scripts · flows · schedules · history</p>
          </div>
        {/if}
      </div>
      <div class="workshop-tabs workshop-tabs-mobile mt-3">
        {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {section === tab.id ? 'workshop-tab-active' : ''}"
            onclick={() => (section = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>
    </header>
    {/if}
    <HistoryPanel
      visible={true}
      {mobile}
      {embedded}
      onOpenFlows={() => (section = "flows")}
    />
  {:else}
  {#if !mobileDetailOpen && !(embedded && lmeHosted)}
    <header class="{embedded ? 'px-3 py-2' : 'workshop-header'}">
      {#if !embedded && !lmeHosted}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 class="text-base font-semibold text-surface-50">Automations</h1>
            <p class="workshop-header-line mt-1">
              Schedules · {counts.enabled}/{counts.total} active
            </p>
          </div>
        </div>
      {/if}

      {#if !lmeHosted}
      <div class="workshop-tabs workshop-tabs-mobile mt-3">
        {#each AUTOMATIONS_SECTIONS as tab (tab.id)}
          <button
            type="button"
            class="workshop-tab {section === tab.id ? 'workshop-tab-active' : ''}"
            onclick={() => (section = tab.id)}
          >
            {tab.label}
          </button>
        {/each}
      </div>
      {/if}

      {#if !embedded}
      <div class="mt-3 flex items-center justify-between gap-2">
        <p class="workshop-header-line">
          Recurring agent turns · delivery in run history
        </p>
        <ScheduleCreatePopover {mobile} {lmeHosted} trigger="primary" />
      </div>

      <label class="cron-search mt-3 block">
        <span class="sr-only">Search automations</span>
        <div class="composer-bar cron-search-bar {mobile ? 'composer-bar-mobile' : ''}">
          <input
            class="cron-search-input"
            type="search"
            placeholder="Search schedules…"
            bind:value={search}
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
          />
        </div>
      </label>
      {/if}
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div
      class="workshop-list-pane min-w-0 flex-1 overflow-y-auto {embedded
        ? 'px-2 py-1'
        : 'mobile-you-scroll px-4 py-3'} {mobileDetailOpen ? 'hidden' : ''}"
    >
      {#if automations.loading && automations.definitions.length === 0}
        <p class="workshop-muted">Loading schedules…</p>
      {:else if automations.error}
        <p class="text-sm text-warning-400">{automations.error}</p>
      {:else if filtered.length === 0}
        <p class="workshop-muted">
          {search.trim()
            ? "No schedules match your search."
            : "No schedules yet. Create one or schedule a specialist from Capabilities."}
        </p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filtered as entry (entry.recurring_id)}
            <li>
              <button
                type="button"
                class="flex w-full flex-col gap-0.5 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {activeRecurringId ===
                entry.recurring_id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => selectEntry(entry)}
              >
                <p class="truncate text-[13px] font-medium tracking-tight text-surface-100">
                  {automations.labelFor(entry)}
                </p>
                <p
                  class="truncate text-[11px] {entry.enabled &&
                  entry.last_run_status === 'failed'
                    ? 'text-warning-400/90'
                    : 'text-surface-500'}"
                >
                  {automations.healthLineFor(entry)}
                </p>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if automations.registerMessage}
        <p class="mt-4 text-xs text-primary-300">{automations.registerMessage}</p>
      {/if}
    </div>

    {#if !lmeHosted}
      <aside
        class="{mobile
          ? mobileDetailOpen
            ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-hidden'
            : 'hidden'
          : 'workshop-detail-pane flex w-[min(420px,42%)] shrink-0 flex-col overflow-hidden border-l border-surface-500/40'}"
      >
        {#if mobileDetailOpen}
          <button
            type="button"
            class="workshop-text-action mx-4 mt-4 mb-1 shrink-0 self-start text-sm"
            onclick={closeMobileDetail}
          >
            ← Back to list
          </button>
        {/if}
        {#if selected}
          <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
            <ScheduleDetailEditor
              entry={selected}
              hideSidebarExpand={true}
              onDeleted={() => {
                selectedId = null;
              }}
            />
          </div>
        {:else}
          <p class="workshop-muted px-4 py-4 text-sm">
            Select a schedule to open it — or create a new one.
          </p>
        {/if}
      </aside>
    {/if}
  </div>

  {#if embedded && !mobileDetailOpen}
    <footer
      class="relative flex shrink-0 items-center gap-1 border-t border-surface-500/25 px-2 py-1.5"
    >
      <div class="min-w-0 flex-1">
        <span class="workshop-faint truncate text-[11px]">
          {counts.enabled}/{counts.total} active
        </span>
      </div>
      <ScheduleCreatePopover {mobile} {lmeHosted} trigger="dock" />
      <div class="relative shrink-0">
        <button
          type="button"
          class="vault-dock-icon-btn {filterActive ? 'vault-dock-icon-btn-active' : ''}"
          aria-haspopup="menu"
          aria-expanded={filterOpen}
          aria-label="Filter schedules"
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
          >
            <div class="px-2.5 pb-1">
              <input
                class="input w-full text-xs"
                type="search"
                placeholder="Search schedules…"
                bind:value={search}
                autocapitalize="off"
                autocorrect="off"
                spellcheck="false"
                onclick={(event) => event.stopPropagation()}
              />
            </div>
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
  {/if}
  {/if}
</section>
