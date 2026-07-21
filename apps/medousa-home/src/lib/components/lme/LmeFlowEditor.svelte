<script lang="ts">
  import FlowComposer from "$lib/components/automations/FlowComposer.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import type { WorkflowListEntry } from "$lib/types/workflow";
  import "$lib/components/automations/flowComposer.css";

  let detailTab = $state<"steps" | "runs">("steps");

  const active = $derived(
    lmeWorkspace.activeTab?.kind === "flow" ? lmeWorkspace.activeTab : null,
  );

  const isDraft = $derived(active?.workflowId === null);

  const selected = $derived.by(() => {
    const id = active?.workflowId;
    if (!id) return null;
    return flows.workflows.find((entry) => entry.workflow_id === id) ?? null;
  });

  const selectedDetail = $derived(
    active?.workflowId ? (flows.detailById[active.workflowId] ?? null) : null,
  );

  const selectedRuns = $derived(
    active?.workflowId ? (flows.runsById[active.workflowId] ?? []) : [],
  );

  $effect(() => {
    const id = active?.workflowId;
    if (!id) return;
    detailTab = "steps";
    void flows.loadDetail(id);
    void flows.loadRuns(id);
  });

  $effect(() => {
    if (!isDraft) return;
    lmeWorkspace.syncFlowComposerTabTitle(flows.composerDraft.name);
  });

  /** After schedule/run closes the composer, drop the draft tab. */
  $effect(() => {
    if (!active || active.workflowId !== null) return;
    if (flows.composerOpen) return;
    void lmeWorkspace.closeTab(active.tabId);
  });

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

  function cancelDraft() {
    flows.closeComposer();
  }

  function duplicateAndEdit() {
    if (!selectedDetail) return;
    lmeWorkspace.openNewFlow({
      name: selectedDetail.name ?? "",
      steps: selectedDetail.steps,
    });
  }
</script>

<div class="lme-flow-editor flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
  {#if !active}
    <p class="px-5 py-5 text-sm text-surface-500 sm:px-7 sm:py-6">
      Select a flow from the side panel.
    </p>
  {:else if isDraft}
    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <FlowComposer bind:draft={flows.composerDraft} onCancel={cancelDraft} />
    </div>
  {:else if !selected && !selectedDetail}
    <div class="overflow-y-auto px-5 py-5 sm:px-7 sm:py-6">
      {#if active.workflowId && flows.detailLoadingId === active.workflowId}
        <p class="text-sm text-surface-500">Loading flow…</p>
      {:else if active.workflowId && flows.detailErrorById[active.workflowId]}
        <p class="text-sm text-warning-400">{flows.detailErrorById[active.workflowId]}</p>
      {:else}
        <p class="text-sm text-surface-500">
          Flow <span class="font-mono text-surface-300">{active.workflowId}</span> not found.
        </p>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface mt-3 self-start"
          onclick={() => void flows.refresh()}
        >
          Refresh
        </button>
      {/if}
    </div>
  {:else}
    {@const entry = selected}
    <div class="mx-auto w-full max-w-2xl overflow-y-auto px-5 py-5 sm:px-7 sm:py-6">
      <p class="text-[11px] tracking-[0.12em] text-surface-500 uppercase">Named flow</p>
      <div class="mt-1 flex flex-wrap items-center gap-2">
        <h2 class="text-xl font-semibold tracking-tight text-surface-50">
          {entry ? flows.labelFor(entry) : active.title}
        </h2>
        {#if entry}
          <WorkshopLivelinessChip variant={statusChipVariant(entry)} />
        {/if}
      </div>
      <p class="workshop-faint mt-1 font-mono text-[11px]">{active.workflowId}</p>

      <div class="workshop-tabs mt-5">
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
        {#if active.workflowId && flows.detailLoadingId === active.workflowId}
          <p class="workshop-muted mt-4 text-sm">Loading flow detail…</p>
        {:else if active.workflowId && flows.detailErrorById[active.workflowId]}
          <p class="mt-4 text-sm text-warning-400">
            {flows.detailErrorById[active.workflowId]}
          </p>
        {:else if selectedDetail}
          <dl class="mt-4 grid gap-3 text-xs sm:grid-cols-2">
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
              <li class="rounded-xl border border-surface-500/30 bg-surface-950/35 px-3 py-2.5 text-xs">
                <p class="font-medium text-surface-100">
                  {index + 1}. {step.kind}
                  <span class="workshop-faint font-mono">· {step.id}</span>
                </p>
                {#if step.kind === "prompt"}
                  <p class="workshop-faint mt-1 whitespace-pre-wrap">{step.user_prompt}</p>
                {:else if step.kind === "grapheme"}
                  <pre class="workshop-faint mt-1 overflow-x-auto font-mono text-[10px]">{step.source}</pre>
                {:else if step.kind === "mcp"}
                  <p class="workshop-faint mt-1 font-mono">
                    {step.server_id}.{step.tool_name}
                  </p>
                {:else if step.kind === "tool_replay"}
                  <p class="workshop-faint mt-1 font-mono">{step.tool_name}</p>
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
          {#if active.workflowId && flows.runsLoadingId === active.workflowId}
            <p class="workshop-muted text-sm">Loading run history…</p>
          {:else if active.workflowId && flows.runsErrorById[active.workflowId]}
            <p class="text-sm text-warning-400">
              {flows.runsErrorById[active.workflowId]}
            </p>
          {:else if selectedRuns.length === 0}
            <p class="workshop-muted text-sm">No runs recorded for this flow yet.</p>
          {:else}
            <ul class="space-y-3">
              {#each selectedRuns as run (run.job_id)}
                <li class="rounded-xl border border-surface-500/30 bg-surface-950/35 p-3">
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

      <div class="mt-6 flex flex-wrap gap-2">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={flows.running || !selectedDetail}
          onclick={() => void rerunSelected()}
        >
          {flows.running ? "Running…" : "Run again"}
        </button>
        {#if selectedDetail}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={duplicateAndEdit}
          >
            Duplicate & edit
          </button>
        {/if}
      </div>

      {#if flows.actionMessage}
        <p class="mt-4 text-xs text-primary-300">{flows.actionMessage}</p>
      {/if}
    </div>
  {/if}
</div>
