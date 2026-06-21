<script lang="ts">
  import { flows } from "$lib/stores/flows.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { toolHistory } from "$lib/stores/toolHistory.svelte";
  import { sliceRefFromRun, type ToolHistoryRunEntry } from "$lib/types/toolHistory";
  import { formatToolName } from "$lib/utils/formatTurn";
  import {
    humanToolRunDetail,
    humanToolRunHeadline,
    suggestFlowNameFromRun,
  } from "$lib/utils/toolHistorySummary";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenFlows?: () => void;
  }

  let { visible, mobile = false, embedded = false, onOpenFlows }: Props = $props();

  let search = $state("");
  let flowName = $state("");

  const filtered = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const rows = [...toolHistory.runs];
    if (!query) return rows;
    return rows.filter((entry) => {
      const haystack = [
        entry.tool_name,
        entry.input_summary,
        entry.session_id,
        entry.slice_id,
        entry.session_preview ?? "",
        humanToolRunHeadline(entry),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  });

  const selectedCount = $derived(toolHistory.selectedIds.size);

  $effect(() => {
    if (!visible) return;
    void toolHistory.refresh({ limit: 120 });
  });

  async function buildFlow(run = false) {
    const response = run
      ? await toolHistory.promoteSelection(flowName, true)
      : await toolHistory.promoteSelection(flowName, false);
    flows.composerDraft = {
      name: response.draft.name ?? flowName,
      goal: "",
      steps: response.draft.steps,
      cron_expr: "0 9 * * *",
      timezone: "UTC",
    };
    flows.lastPlan = null;
    flows.composerOpen = true;
    flowDraft.clear();
    onOpenFlows?.();
  }

  async function automateEntry(entry: ToolHistoryRunEntry) {
    const name = suggestFlowNameFromRun(entry);
    const response = await toolHistory.promoteRef(sliceRefFromRun(entry), name);
    flows.composerDraft = {
      name: response.draft.name ?? name,
      goal: settings.showWorkshopGuidance
        ? `Repeat: ${humanToolRunHeadline(entry)}`
        : "",
      steps: response.draft.steps,
      cron_expr: "0 9 * * *",
      timezone: "UTC",
    };
    flows.lastPlan = null;
    flows.composerOpen = true;
    onOpenFlows?.();
  }
</script>

<section
  class="automations-history flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden {visible
    ? ''
    : 'hidden'}"
>
  <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
    {#if !embedded}
      <p class="workshop-header-line">
        Tool runs from chat — promote to a flow
      </p>
    {/if}

    <label class="cron-search mt-3 block">
      <span class="sr-only">Search activity</span>
      <div class="composer-bar cron-search-bar {mobile ? 'composer-bar-mobile' : ''}">
        <input
          class="cron-search-input"
          type="search"
          placeholder="Search tools, args, slice ids…"
          bind:value={search}
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
        />
      </div>
    </label>

    {#if selectedCount > 0}
      <div class="mt-3 flex flex-wrap items-end gap-2">
        <label class="cron-field min-w-[10rem] flex-1">
          <span class="cron-field-label">Flow name</span>
          <div class="composer-bar cron-field-bar cron-field-bar-compact">
            <input
              class="cron-field-input"
              bind:value={flowName}
              placeholder="Repeat Tuesday research"
              spellcheck="false"
            />
          </div>
        </label>
        <button
          type="button"
          class="btn btn-sm variant-soft-primary"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(false)}
        >
          {toolHistory.promoting ? "Building…" : `Edit flow (${selectedCount})`}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(true)}
        >
          Run now
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => toolHistory.clearSelection()}
        >
          Clear
        </button>
      </div>
    {/if}
  </header>

  <div class="mobile-you-scroll min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-4 py-3">
    {#if toolHistory.loading && toolHistory.runs.length === 0}
      <p class="workshop-muted">Loading recent activity…</p>
    {:else if toolHistory.error}
      <p class="text-sm text-warning-400">{toolHistory.error}</p>
    {:else if filtered.length === 0}
      <p class="workshop-muted">
        {search.trim()
          ? "Nothing matches that search."
          : "No tool runs yet. Successful chat tools appear here for replay and flow promotion."}
      </p>
    {:else}
      <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
        {#each filtered as entry (entry.entry_id)}
          <li class="min-w-0 px-2 py-3">
            <div class="flex min-w-0 items-start gap-3">
              <input
                type="checkbox"
                class="mt-1 shrink-0"
                checked={toolHistory.selectedIds.has(entry.entry_id)}
                onchange={() => toolHistory.toggleSelected(entry.entry_id)}
              />
              <div class="min-w-0 flex-1 overflow-hidden">
                <div class="flex min-w-0 flex-wrap items-center gap-2">
                  <p class="min-w-0 truncate text-sm font-medium leading-snug text-surface-50">
                    {formatToolName(entry.tool_name)}
                  </p>
                  <span
                    class="text-[10px] uppercase tracking-wide {entry.status === 'succeeded'
                      ? 'text-success-400/90'
                      : entry.status === 'failed'
                        ? 'text-error-400'
                        : 'text-surface-500'}"
                  >
                    {toolHistory.statusLabel(entry.status)}
                  </span>
                  {#if entry.redacted}
                    <span class="rounded bg-warning-500/15 px-1.5 py-0.5 text-[10px] text-warning-300">
                      redacted
                    </span>
                  {/if}
                </div>
                {#if settings.showWorkshopGuidance}
                  <p class="workshop-faint mt-1 line-clamp-2 text-[11px] leading-snug">
                    {humanToolRunHeadline(entry)}
                  </p>
                {/if}
                <p class="workshop-faint mt-1 truncate font-mono text-[10px]">
                  {humanToolRunDetail(entry)}
                </p>
                <p class="workshop-faint mt-0.5 text-[10px] text-surface-500">
                  {toolHistory.formatTimestamp(entry.timestamp)}
                </p>
                {#if entry.input_summary.trim()}
                  <details class="mt-2">
                    <summary class="workshop-text-action cursor-pointer text-[10px]">
                      Input
                    </summary>
                    <p class="workshop-faint mt-1 break-all font-mono text-[10px]">
                      {entry.input_summary}
                    </p>
                  </details>
                {/if}
                {#if entry.status === "succeeded"}
                  <button
                    type="button"
                    class="workshop-text-action mt-2 text-[11px]"
                    disabled={toolHistory.promoting}
                    onclick={() => void automateEntry(entry)}
                  >
                    → Flow
                  </button>
                {/if}
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if toolHistory.actionMessage}
      <p class="mt-4 text-xs text-primary-300">{toolHistory.actionMessage}</p>
    {/if}
  </div>
</section>
