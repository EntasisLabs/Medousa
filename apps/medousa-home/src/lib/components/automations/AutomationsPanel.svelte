<script lang="ts">
  import AutomationCreateForm from "$lib/components/automations/AutomationCreateForm.svelte";
  import FlowsPanel from "$lib/components/automations/FlowsPanel.svelte";
  import HistoryPanel from "$lib/components/automations/HistoryPanel.svelte";
  import ScriptsWorkbenchPanel from "$lib/components/automations/ScriptsWorkbenchPanel.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import { AUTOMATIONS_SECTIONS } from "$lib/automationsSections";
  import { browserTimezone } from "$lib/utils/friendlySchedule";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { automationsNav, type AutomationsSection } from "$lib/stores/automationsNav.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import type {
    AutomationDeliveryMode,
    RecurringDefinitionEntry,
  } from "$lib/types/recurring";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  let section = $state<AutomationsSection>("scripts");

  let search = $state("");
  let selectedId = $state<string | null>(null);
  let detailTab = $state<"schedule" | "runs" | "delivery">("schedule");

  const mobileDetailOpen = $derived(
    mobile && (selectedId !== null || automationDraft.showCreate),
  );
  let confirmDeleteId = $state<string | null>(null);

  let createTitle = $state("");
  let createPrompt = $state("");
  let createCron = $state("0 9 * * *");
  let createTimezone = $state(browserTimezone());
  let createManuscript = $state<string | undefined>(undefined);
  let createDeliveryMode = $state<AutomationDeliveryMode>("in_app");
  let createTelegramChatId = $state("");

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
    selectedId
      ? (automations.definitions.find((entry) => entry.recurring_id === selectedId) ??
        null)
      : null,
  );

  const selectedRuns = $derived(
    selected ? (automations.runsById[selected.recurring_id] ?? []) : [],
  );

  $effect(() => {
    if (!visible) return;
    const pending = automationsNav.consumeSection();
    if (pending) section = pending;
  });

  $effect(() => {
    if (!visible || !flowDraft.openComposer || flowDraft.pendingRefs.length === 0) return;
    void flows
      .applyFromSliceRefs(flowDraft.pendingRefs, flowDraft.seedDraft.name)
      .finally(() => {
        section = "flows";
        flowDraft.clear();
      });
  });

  $effect(() => {
    if (!visible) return;
    if (section === "schedules") {
      void automations.refresh();
    }
  });

  $effect(() => {
    if (!visible || !automationDraft.showCreate || !automationDraft.createDraft) return;
    createTitle = automationDraft.createDraft.display_name ?? "";
    createPrompt = automationDraft.createDraft.prompt;
    createCron = automationDraft.createDraft.cron_expr;
    createTimezone = automationDraft.createDraft.timezone ?? "UTC";
    createManuscript = automationDraft.createDraft.manuscript_id;
    createDeliveryMode = automationDraft.createDraft.delivery_mode ?? "in_app";
    createTelegramChatId = automationDraft.createDraft.telegram_chat_id ?? "";
    selectedId = null;
    detailTab = "schedule";
  });

  $effect(() => {
    const id = selected?.recurring_id;
    if (!visible || !id) return;
    void automations.loadRuns(id);
  });

  function openNew() {
    automationDraft.openCreate();
    detailTab = "schedule";
  }

  async function submitCreate() {
    await automations.register({
      display_name: createTitle.trim() || undefined,
      prompt: createPrompt.trim() || "Scheduled task",
      cron_expr: createCron.trim() || "0 9 * * *",
      manuscript_id: createManuscript,
      timezone: createTimezone.trim() || "UTC",
      model_hint: runtime.model,
      execution_mode: "agent_turn",
      delivery_mode: createDeliveryMode,
      telegram_chat_id: createTelegramChatId,
    });
    automationDraft.clearCreate();
  }

  function selectEntry(entry: RecurringDefinitionEntry) {
    selectedId = entry.recurring_id;
    automationDraft.clearCreate();
    confirmDeleteId = null;
    detailTab = "schedule";
  }

  function statusChipVariant(entry: RecurringDefinitionEntry): "scheduled" | "paused" | "running" {
    if (!entry.enabled) return "paused";
    if (entry.last_run_status === "failed") return "running";
    return "scheduled";
  }

  function closeMobileDetail() {
    selectedId = null;
    automationDraft.clearCreate();
    confirmDeleteId = null;
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
    <ScriptsWorkbenchPanel visible={true} {mobile} {embedded} />
  {:else if section === "flows"}
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
    <FlowsPanel visible={true} {mobile} {embedded} />
  {:else if section === "history"}
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
    <HistoryPanel
      visible={true}
      {mobile}
      {embedded}
      onOpenFlows={() => (section = "flows")}
    />
  {:else}
  {#if !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h1 class="text-base font-semibold text-surface-50">Automations</h1>
            <p class="workshop-header-line mt-1">
              Schedules · {counts.enabled}/{counts.total} active
            </p>
          </div>
        </div>
      {/if}

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

      <div class="mt-3 flex items-center justify-between gap-2">
        {#if embedded}
          <p class="workshop-faint text-xs">
            {counts.enabled}/{counts.total} active
          </p>
        {:else}
          <p class="workshop-header-line">
            Recurring agent turns · delivery in run history
          </p>
        {/if}
        <button
          type="button"
          class="btn btn-sm shrink-0 variant-filled-primary"
          onclick={openNew}
        >
          {embedded ? "+ New" : "+ New schedule"}
        </button>
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
    </header>
  {/if}

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div
      class="workshop-list-pane mobile-you-scroll min-w-0 flex-1 overflow-y-auto px-4 py-3 {mobileDetailOpen
        ? 'hidden'
        : ''}"
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
                class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedId ===
                entry.recurring_id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => selectEntry(entry)}
              >
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <p class="truncate font-medium text-surface-100">
                      {automations.labelFor(entry)}
                    </p>
                    <WorkshopLivelinessChip variant={statusChipVariant(entry)} />
                    <span class="workshop-faint">{automations.originFor(entry)}</span>
                  </div>
                  <p class="workshop-faint mt-0.5 truncate font-mono text-[11px]">
                    {entry.cron_expr} · {entry.timezone}
                  </p>
                  <p class="workshop-faint mt-1 truncate text-[11px]">
                    {automations.deliveryLabelFor(entry)}
                    {#if entry.last_run_status}
                      · Last {automations.statusLabel(entry.last_run_status)}
                    {/if}
                  </p>
                </div>
                <div class="shrink-0 text-right text-[11px] text-surface-400">
                  <p>Next {automations.formatNextRun(entry.next_run_at_utc)}</p>
                  <p class="mt-0.5">
                    Last {entry.last_run_at_utc
                      ? automations.formatNextRun(entry.last_run_at_utc)
                      : "—"}
                  </p>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if automations.registerMessage}
        <p class="mt-4 text-xs text-primary-300">{automations.registerMessage}</p>
      {/if}
    </div>

    <aside
      class="{mobile
        ? mobileDetailOpen
          ? 'mobile-you-scroll flex min-h-0 flex-1 flex-col overflow-y-auto'
          : 'hidden'
        : 'workshop-detail-pane w-[min(420px,42%)] shrink-0 overflow-y-auto border-l border-surface-500/40'} px-4 py-4"
    >
      {#if mobileDetailOpen}
        <button
          type="button"
          class="workshop-text-action mb-3 shrink-0 text-sm"
          onclick={() => {
            closeMobileDetail();
          }}
        >
          ← Back to list
        </button>
      {/if}
      {#if automationDraft.showCreate}
        <h2 class="workshop-section-title">New schedule</h2>
        <p class="workshop-faint mt-1 text-xs">Agent turn with tools · results in run history.</p>
        <AutomationCreateForm
          {mobile}
          bind:title={createTitle}
          bind:prompt={createPrompt}
          bind:cronExpr={createCron}
          bind:timezone={createTimezone}
          bind:deliveryMode={createDeliveryMode}
          bind:telegramChatId={createTelegramChatId}
          manuscript={createManuscript}
          registering={automations.registering}
          onCancel={() => automationDraft.clearCreate()}
          onSubmit={submitCreate}
        />
      {:else if selected}
        <h2 class="workshop-section-title">{automations.labelFor(selected)}</h2>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selected.recurring_id}</p>

        <div class="workshop-tabs mt-4">
          {#each [
            { id: "schedule", label: "Schedule" },
            { id: "runs", label: "Runs" },
            { id: "delivery", label: "Delivery" },
          ] as tab (tab.id)}
            <button
              type="button"
              class="workshop-tab {detailTab === tab.id ? 'workshop-tab-active' : ''}"
              onclick={() => (detailTab = tab.id as typeof detailTab)}
            >
              {tab.label}
            </button>
          {/each}
        </div>

        {#if detailTab === "schedule"}
          <dl class="mt-4 space-y-2 text-xs">
            <div>
              <dt class="workshop-label">Status</dt>
              <dd class="mt-0.5">
                <WorkshopLivelinessChip variant={statusChipVariant(selected)} />
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Origin</dt>
              <dd class="mt-0.5 text-surface-200">{automations.originFor(selected)}</dd>
            </div>
            <div>
              <dt class="workshop-label">Execution</dt>
              <dd class="mt-0.5 text-surface-200">
                {selected.execution_mode === "prompt" ? "Quick prompt only" : "Agent turn"}
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Cron</dt>
              <dd class="mt-0.5 font-mono text-surface-200">
                {selected.cron_expr} · {selected.timezone}
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Next run</dt>
              <dd class="mt-0.5 text-surface-200">
                {automations.formatNextRun(selected.next_run_at_utc)}
              </dd>
            </div>
            <div>
              <dt class="workshop-label">Last run</dt>
              <dd class="mt-0.5 text-surface-200">
                {selected.last_run_at_utc
                  ? automations.formatNextRun(selected.last_run_at_utc)
                  : "—"}
                {#if selected.last_run_status}
                  · {automations.statusLabel(selected.last_run_status)}
                {/if}
              </dd>
            </div>
            {#if selected.prompt_excerpt}
              <div>
                <dt class="workshop-label">Prompt</dt>
                <dd class="mt-0.5 text-surface-300">{selected.prompt_excerpt}</dd>
              </div>
            {/if}
          </dl>
        {:else if detailTab === "runs"}
          <div class="mt-4 space-y-3">
            {#if automations.runsLoadingId === selected.recurring_id}
              <p class="workshop-muted text-sm">Loading run history…</p>
            {:else if automations.runsErrorById[selected.recurring_id]}
              <p class="text-sm text-warning-400">
                {automations.runsErrorById[selected.recurring_id]}
              </p>
            {:else if selectedRuns.length === 0}
              <p class="workshop-muted text-sm">No runs yet. The next tick will appear here.</p>
            {:else}
              <ul class="space-y-3">
                {#each selectedRuns as run (run.job_id)}
                  <li class="workshop-inset p-3">
                    <div class="flex items-start justify-between gap-2">
                      <div>
                        <p class="text-sm font-medium text-surface-100">
                          {automations.statusLabel(run.status)}
                        </p>
                        <p class="workshop-faint mt-0.5 font-mono text-[10px]">
                          {run.job_id}
                        </p>
                      </div>
                      <p class="workshop-faint shrink-0 text-[11px]">
                        {automations.formatTimestamp(run.updated_at_utc)}
                      </p>
                    </div>
                    {#if run.output_text}
                      <div class="prose-workshop mt-2 max-h-48 overflow-y-auto text-sm">
                        <MarkdownContent content={run.output_text} />
                      </div>
                    {:else if run.is_terminal}
                      <p class="workshop-muted mt-2 text-xs">No output text recorded.</p>
                    {:else}
                      <p class="workshop-muted mt-2 text-xs">Run still in progress…</p>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {:else}
          <dl class="mt-4 space-y-2 text-xs">
            <div>
              <dt class="workshop-label">Destination</dt>
              <dd class="mt-0.5 text-surface-200">
                {automations.deliveryLabelFor(selected)}
              </dd>
            </div>
          </dl>
          <p class="workshop-faint mt-4 text-xs">
            Delivery is set at creation time for now. Editing existing delivery lands in the
            next patch.
          </p>
        {/if}

        <div class="mt-5 flex flex-wrap gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            disabled={automations.updatingId === selected.recurring_id}
            onclick={() =>
              void automations.setEnabled(selected.recurring_id, !selected.enabled)}
          >
            {selected.enabled ? "Pause" : "Resume"}
          </button>
          {#if confirmDeleteId === selected.recurring_id}
            <button
              type="button"
              class="btn btn-sm variant-filled-error"
              disabled={automations.deletingId === selected.recurring_id}
              onclick={async () => {
                await automations.remove(selected.recurring_id);
                selectedId = null;
                confirmDeleteId = null;
              }}
            >
              Confirm delete
            </button>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() => (confirmDeleteId = null)}
            >
              Cancel
            </button>
          {:else}
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface text-error-400"
              onclick={() => (confirmDeleteId = selected.recurring_id)}
            >
              Delete…
            </button>
          {/if}
        </div>
      {:else}
        <p class="workshop-muted text-sm">
          Select a schedule to view runs, delivery, and controls — or create a new one.
        </p>
      {/if}
    </aside>
  </div>
  {/if}
</section>
