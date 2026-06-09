<script lang="ts">
  import type { ToolRunState } from "$lib/types/chat";
  import { formatToolName } from "$lib/utils/formatTurn";

  interface Props {
    runs: ToolRunState[];
    compact?: boolean;
    /** Collapse the tool timeline until expanded (default for completed turns). */
    inspectorCollapsed?: boolean;
  }

  let { runs, compact = false, inspectorCollapsed = true }: Props = $props();

  const grouped = $derived(groupRunsByRound(runs));
  const roundCount = $derived(grouped.size);
  const toolCount = $derived(runs.length);
  const latestRound = $derived(
    grouped.size > 0 ? Math.max(...grouped.keys()) : 1,
  );

  function groupRunsByRound(items: ToolRunState[]): Map<number, ToolRunState[]> {
    const map = new Map<number, ToolRunState[]>();
    for (const run of items) {
      const bucket = map.get(run.round) ?? [];
      bucket.push(run);
      map.set(run.round, bucket);
    }
    return new Map([...map.entries()].sort(([a], [b]) => a - b));
  }

  function statusClass(status: ToolRunState["status"]): string {
    switch (status) {
      case "running":
        return "border-primary-500/40 bg-primary-500/10 text-primary-200";
      case "failed":
        return "border-rose-500/40 bg-rose-500/10 text-rose-200";
      default:
        return "border-surface-600 bg-surface-800/80 text-surface-200";
    }
  }

  function statusDot(status: ToolRunState["status"]): string {
    switch (status) {
      case "running":
        return "bg-primary-400 animate-pulse";
      case "failed":
        return "bg-rose-400";
      default:
        return "bg-emerald-400";
    }
  }

  function roundLabel(round: number, roundRuns: ToolRunState[]): string {
    const names = roundRuns.map((run) => formatToolName(run.toolName)).join(" · ");
    return `Round ${round} · ${roundRuns.length} tool${roundRuns.length === 1 ? "" : "s"} · ${names}`;
  }

  function inspectorSummary(): string {
    const roundWord = roundCount === 1 ? "round" : "rounds";
    const toolWord = toolCount === 1 ? "tool" : "tools";
    return `Tools · ${roundCount} ${roundWord} · ${toolCount} ${toolWord}`;
  }
</script>

{#snippet chipList(roundRuns: ToolRunState[])}
  <div class="flex flex-wrap gap-1.5">
    {#each roundRuns as run (run.runId)}
      <details class="group rounded-full border px-2 py-1 {statusClass(run.status)}">
        <summary
          class="flex cursor-pointer list-none items-center gap-1.5 marker:content-none"
        >
          <span class="inline-block h-1.5 w-1.5 rounded-full {statusDot(run.status)}"></span>
          <span class="font-medium">{formatToolName(run.toolName)}</span>
          {#if run.outputSummary && !compact}
            <span class="workshop-faint hidden max-w-[12rem] truncate sm:inline">
              · {run.outputSummary}
            </span>
          {/if}
        </summary>
        {#if run.inputSummary || run.outputSummary || (run.artifactRefs?.length ?? 0) > 0}
          <div class="mt-2 space-y-1 border-t border-white/10 pt-2 text-[11px] leading-relaxed">
            {#if run.inputSummary}
              <p><span class="text-surface-500">In:</span> {run.inputSummary}</p>
            {/if}
            {#if run.outputSummary}
              <p><span class="text-surface-500">Out:</span> {run.outputSummary}</p>
            {/if}
            {#if run.artifactRefs && run.artifactRefs.length > 0}
              <p class="text-surface-500">
                {run.artifactRefs.length} artifact receipt{run.artifactRefs.length === 1
                  ? ""
                  : "s"}
              </p>
            {/if}
          </div>
        {/if}
      </details>
    {/each}
  </div>
{/snippet}

{#snippet roundGroups(collapsePastRounds: boolean)}
  <div class="space-y-2 {compact ? 'text-[10px]' : 'text-xs'}">
    {#each [...grouped.entries()] as [round, roundRuns] (round)}
      {#if grouped.size > 1 && collapsePastRounds && round !== latestRound}
        <details class="rounded-md border border-surface-700/60 bg-surface-900/40 px-2 py-1.5">
          <summary class="workshop-faint cursor-pointer select-none uppercase tracking-wide">
            {roundLabel(round, roundRuns)}
          </summary>
          <div class="mt-2">
            {@render chipList(roundRuns)}
          </div>
        </details>
      {:else if grouped.size > 1}
        <div>
          <p class="workshop-faint mb-1 uppercase tracking-wide">Round {round}</p>
          {@render chipList(roundRuns)}
        </div>
      {:else}
        {@render chipList(roundRuns)}
      {/if}
    {/each}
  </div>
{/snippet}

{#if runs.length > 0}
  {#if inspectorCollapsed && !compact}
    <details class="rounded-md border border-surface-700/50 bg-surface-900/30 px-2 py-1.5 text-xs">
      <summary class="workshop-faint cursor-pointer select-none font-medium text-surface-300">
        {inspectorSummary()}
      </summary>
      <div class="mt-2">
        {@render roundGroups(true)}
      </div>
    </details>
  {:else}
    {@render roundGroups(!inspectorCollapsed && roundCount > 1)}
  {/if}
{/if}
