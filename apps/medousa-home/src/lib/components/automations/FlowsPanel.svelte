<script lang="ts">
  import FlowComposer from "$lib/components/automations/FlowComposer.svelte";
  import GraphemeRecipeCards from "$lib/components/grapheme/GraphemeRecipeCards.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { settings } from "$lib/stores/settings.svelte";
  import type { GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import type { WorkflowListEntry } from "$lib/types/workflow";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  let search = $state("");
  let selectedId = $state<string | null>(null);
  let detailTab = $state<"steps" | "runs">("steps");

  const mobileDetailOpen = $derived(
    mobile && (selectedId !== null || flows.composerOpen),
  );

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

  const selected = $derived(
    selectedId
      ? (flows.workflows.find((entry) => entry.workflow_id === selectedId) ?? null)
      : null,
  );

  const selectedDetail = $derived(
    selected ? (flows.detailById[selected.workflow_id] ?? null) : null,
  );

  const selectedRuns = $derived(
    selected ? (flows.runsById[selected.workflow_id] ?? []) : [],
  );

  $effect(() => {
    if (!visible) return;
    void flows.refresh();
  });

  $effect(() => {
    const id = selected?.workflow_id;
    if (!visible || !id) return;
    void flows.loadDetail(id);
    void flows.loadRuns(id);
  });

  function selectEntry(entry: WorkflowListEntry) {
    selectedId = entry.workflow_id;
    flows.closeComposer();
    detailTab = "steps";
  }

  function statusChipVariant(entry: WorkflowListEntry): "scheduled" | "paused" | "running" {
    if (entry.status === "failed") return "running";
    if (entry.status === "running" || entry.status === "enqueued") return "running";
    if (entry.status === "canceled") return "paused";
    return "scheduled";
  }

  async function rerunSelected() {
    if (!selectedDetail) return;
    await flows.rerun({
      name: selectedDetail.name,
      strategy: selectedDetail.strategy,
      mode: selectedDetail.mode,
      on_failure: selectedDetail.on_failure,
      steps: selectedDetail.steps,
      queue: "default",
    });
  }

  function startFlowFromRecipe(recipe: GraphemeRecipe) {
    flows.openComposerWithRecipe(recipe);
  }

  function closeMobileDetail() {
    selectedId = null;
    flows.closeComposer();
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

<section
  class="cron-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {mobile
    ? 'cron-panel-mobile'
    : ''} {visible ? '' : 'hidden'}"
>
  {#if !mobileDetailOpen}
    <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
      {#if !embedded}
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p class="workshop-header-line">
              Workflows — run once or on a schedule
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            onclick={() => flows.openComposer()}
          >
            + New flow
          </button>
        </div>
      {:else}
        <div class="flex items-center justify-between gap-2">
          <p class="workshop-faint text-xs">{flows.workflows.length} flows</p>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            onclick={() => flows.openComposer()}
          >
            + New
          </button>
        </div>
      {/if}

      <label class="cron-search mt-3 block">
        <span class="sr-only">Search flows</span>
        <div class="composer-bar cron-search-bar {mobile ? 'composer-bar-mobile' : ''}">
          <input
            class="cron-search-input"
            type="search"
            placeholder="Search flows…"
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
      {#if flows.loading && flows.workflows.length === 0}
        <p class="workshop-muted">Loading flows…</p>
      {:else if flows.error}
        <p class="text-sm text-warning-400">{flows.error}</p>
      {:else if filtered.length === 0}
        <div class="space-y-4">
          <p class="text-sm text-surface-200">
            {search.trim()
              ? "No flows match your search."
              : "No flows yet."}
          </p>
          {#if !search.trim() && settings.showWorkshopGuidance}
            <GraphemeRecipeCards compact onselect={startFlowFromRecipe} />
            <button
              type="button"
              class="btn btn-sm variant-soft-primary"
              onclick={() => flows.openComposer()}
            >
              New flow
            </button>
          {:else if !search.trim()}
            <button
              type="button"
              class="btn btn-sm variant-soft-primary"
              onclick={() => flows.openComposer()}
            >
              New flow
            </button>
          {/if}
        </div>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
          {#each filtered as entry (entry.workflow_id)}
            <li>
              <button
                type="button"
                class="flex w-full items-start gap-3 px-2 py-2.5 text-left transition hover:bg-surface-800/70 {selectedId ===
                entry.workflow_id
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => selectEntry(entry)}
              >
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <p class="truncate font-medium text-surface-100">
                      {flows.labelFor(entry)}
                    </p>
                    <WorkshopLivelinessChip variant={statusChipVariant(entry)} />
                  </div>
                  <p class="workshop-faint mt-0.5 truncate font-mono text-[11px]">
                    {entry.step_count} steps · {entry.strategy}
                  </p>
                  {#if entry.scheduled_recurring_id}
                    <p class="workshop-faint mt-1 truncate text-[11px]">
                      Scheduled · {entry.scheduled_recurring_id}
                    </p>
                  {/if}
                </div>
                <div class="shrink-0 text-right text-[11px] text-surface-400">
                  <p>{flows.statusLabel(entry.status)}</p>
                  <p class="mt-0.5">{flows.formatTimestamp(entry.created_at_utc)}</p>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if flows.actionMessage && !flows.composerOpen}
        <p class="mt-4 text-xs text-primary-300">{flows.actionMessage}</p>
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
          onclick={closeMobileDetail}
        >
          ← Back to list
        </button>
      {/if}

      {#if flows.composerOpen}
        <h2 class="workshop-section-title">New flow</h2>
        <p class="workshop-faint mt-1 text-xs">
          Name it, add steps, try it, then schedule it.
        </p>
        <div class="mt-4">
          <FlowComposer
            {mobile}
            bind:draft={flows.composerDraft}
            onCancel={() => flows.closeComposer()}
          />
        </div>
      {:else if selected}
        <h2 class="workshop-section-title">{flows.labelFor(selected)}</h2>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{selected.workflow_id}</p>

        <div class="workshop-tabs workshop-tabs-mobile mt-4">
          {#each [
            { id: "steps", label: "Steps" },
            { id: "runs", label: "Runs" },
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

        {#if detailTab === "steps"}
          {#if flows.detailLoadingId === selected.workflow_id}
            <p class="workshop-muted mt-4 text-sm">Loading flow detail…</p>
          {:else if flows.detailErrorById[selected.workflow_id]}
            <p class="mt-4 text-sm text-warning-400">
              {flows.detailErrorById[selected.workflow_id]}
            </p>
          {:else if selectedDetail}
            <dl class="mt-4 space-y-2 text-xs">
              <div>
                <dt class="workshop-label">Status</dt>
                <dd class="mt-0.5">
                  <WorkshopLivelinessChip variant={statusChipVariant(selected)} />
                </dd>
              </div>
              <div>
                <dt class="workshop-label">Strategy</dt>
                <dd class="mt-0.5 text-surface-200">{selectedDetail.strategy}</dd>
              </div>
              {#if selectedDetail.scheduled_recurring_id}
                <div>
                  <dt class="workshop-label">Schedule</dt>
                  <dd class="mt-0.5 font-mono text-surface-200">
                    {selectedDetail.scheduled_recurring_id}
                  </dd>
                </div>
              {/if}
            </dl>

            <ul class="mt-4 space-y-2">
              {#each selectedDetail.steps as step, index (step.id)}
                <li class="workshop-inset p-3 text-xs">
                  <p class="font-medium text-surface-100">
                    {index + 1}. {step.kind}
                    <span class="workshop-faint font-mono">· {step.id}</span>
                  </p>
                  {#if step.kind === "prompt"}
                    <p class="workshop-faint mt-1 whitespace-pre-wrap">{step.user_prompt}</p>
                  {:else if step.kind === "grapheme"}
                    <pre class="workshop-faint mt-1 overflow-x-auto font-mono text-[10px]">{step.source}</pre>
                  {:else}
                    <p class="workshop-faint mt-1 font-mono">
                      {step.server_id}.{step.tool_name}
                    </p>
                  {/if}
                  {#if selectedDetail.step_results.length > 0}
                    {@const result = selectedDetail.step_results.find((row) => row.id === step.id)}
                    {#if result}
                      <p class="mt-1 text-primary-300">
                        {flows.statusLabel(result.status)}
                        {#if result.error}
                          · {result.error}
                        {/if}
                      </p>
                    {/if}
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          <div class="mt-4 space-y-3">
            {#if flows.runsLoadingId === selected.workflow_id}
              <p class="workshop-muted text-sm">Loading run history…</p>
            {:else if flows.runsErrorById[selected.workflow_id]}
              <p class="text-sm text-warning-400">
                {flows.runsErrorById[selected.workflow_id]}
              </p>
            {:else if selectedRuns.length === 0}
              <p class="workshop-muted text-sm">No runs recorded for this flow yet.</p>
            {:else}
              <ul class="space-y-3">
                {#each selectedRuns as run (run.job_id)}
                  <li class="workshop-inset p-3">
                    <div class="flex items-start justify-between gap-2">
                      <div>
                        <p class="text-sm font-medium text-surface-100">
                          {flows.statusLabel(run.status)}
                        </p>
                        <p class="workshop-faint mt-0.5 font-mono text-[10px]">
                          {run.job_id}
                        </p>
                      </div>
                      <p class="workshop-faint shrink-0 text-[11px]">
                        {flows.formatTimestamp(run.updated_at_utc)}
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
        {/if}

        <div class="mt-5 flex flex-wrap gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            disabled={flows.running || !selectedDetail}
            onclick={() => void rerunSelected()}
          >
            {flows.running ? "Running…" : "Run again"}
          </button>
          {#if selectedDetail}
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={() =>
                flows.openComposer({
                  name: selectedDetail.name ?? "",
                  steps: selectedDetail.steps,
                })}
            >
              Duplicate & edit
            </button>
          {/if}
        </div>
      {:else}
        <p class="workshop-muted text-sm">
          Pick a flow on the left to see what it does — or use + New flow to start fresh.
        </p>
      {/if}
    </aside>
  </div>
</section>
