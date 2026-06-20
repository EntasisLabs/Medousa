<script lang="ts">
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { toolHistory } from "$lib/stores/toolHistory.svelte";
  import { formatToolName } from "$lib/utils/formatTurn";

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
</script>

<section
  class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}"
>
  <header class="{embedded ? 'border-b border-surface-500/40 px-4 py-3' : 'workshop-header'}">
    {#if !embedded}
      <p class="workshop-header-line">
        Receipt-grade tool runs across sessions — promote to flows
      </p>
    {/if}

    <label class="cron-search mt-3 block">
      <span class="sr-only">Search tool history</span>
      <div class="composer-bar cron-search-bar {mobile ? 'composer-bar-mobile' : ''}">
        <input
          class="cron-search-input"
          type="search"
          placeholder="Search tools, sessions, slices…"
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
              placeholder="Replay Tuesday research"
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

  <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-4 py-3">
    {#if toolHistory.loading && toolHistory.runs.length === 0}
      <p class="workshop-muted">Loading tool history…</p>
    {:else if toolHistory.error}
      <p class="text-sm text-warning-400">{toolHistory.error}</p>
    {:else if filtered.length === 0}
      <p class="workshop-muted">
        {search.trim()
          ? "No tool runs match your search."
          : "No indexed tool runs yet. Use tools in chat — runs appear here for replay."}
      </p>
    {:else}
      <ul class="divide-y divide-surface-500/35 border-y border-surface-500/35">
        {#each filtered as entry (entry.entry_id)}
          <li>
            <label class="flex cursor-pointer items-start gap-3 px-2 py-2.5 hover:bg-surface-800/70">
              <input
                type="checkbox"
                class="mt-1"
                checked={toolHistory.selectedIds.has(entry.entry_id)}
                onchange={() => toolHistory.toggleSelected(entry.entry_id)}
              />
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                  <p class="font-medium text-surface-100">
                    {formatToolName(entry.tool_name)}
                  </p>
                  <span class="workshop-faint">{toolHistory.statusLabel(entry.status)}</span>
                  {#if entry.redacted}
                    <span class="rounded bg-warning-500/15 px-1.5 py-0.5 text-[10px] text-warning-300">
                      redacted
                    </span>
                  {/if}
                </div>
                <p class="workshop-faint mt-0.5 truncate text-[11px]">
                  {entry.input_summary}
                </p>
                <p class="workshop-faint mt-1 font-mono text-[10px]">
                  {entry.slice_id} · {entry.session_id}
                </p>
              </div>
              <p class="workshop-faint shrink-0 text-[11px]">
                {toolHistory.formatTimestamp(entry.timestamp)}
              </p>
            </label>
          </li>
        {/each}
      </ul>
    {/if}

    {#if toolHistory.actionMessage}
      <p class="mt-4 text-xs text-primary-300">{toolHistory.actionMessage}</p>
    {/if}
  </div>
</section>
