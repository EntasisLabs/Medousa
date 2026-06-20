<script lang="ts">
  import { ChevronDown, Route, Workflow } from "@lucide/svelte";
  import type { ToolRunState } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import { sliceRefFromChatToolRun } from "$lib/types/toolHistory";
  import type { ToolLineageSegment } from "$lib/utils/toolRunLineage";
  import {
    buildToolLineage,
    formatCollapsedLabel,
    formatLineagePreview,
    formatSegmentLabel,
    segmentAccentClass,
    segmentLabelClass,
  } from "$lib/utils/toolRunLineage";

  interface Props {
    runs: ToolRunState[];
    sessionId?: string;
    turnIndex?: number | null;
    onPromoteToFlow?: (ref: ToolHistorySliceRef) => void | Promise<void>;
    compact?: boolean;
    inspectorCollapsed?: boolean;
  }

  let {
    runs,
    sessionId,
    turnIndex = null,
    onPromoteToFlow,
    compact = false,
    inspectorCollapsed = true,
  }: Props = $props();

  const lineage = $derived(buildToolLineage(runs));
  const toolCount = $derived(runs.length);
  const collapsed = $derived(formatCollapsedLabel(lineage, toolCount));
  const fullTrace = $derived(formatLineagePreview(lineage));
  const hasRunning = $derived(runs.some((run) => run.status === "running"));
  const activeSegment = $derived(
    hasRunning ? lineage.find((segment) => segment.status === "running") : null,
  );
  const isDone = $derived(!hasRunning && runs.every((run) => run.status !== "failed"));
  const footnote = $derived(inspectorCollapsed && isDone);

  function segmentHasDetail(segment: ToolLineageSegment): boolean {
    if (segment.count > 1) return true;
    const run = segment.runs[0];
    return Boolean(
      run.inputSummary?.trim() ||
        run.outputSummary?.trim() ||
        (run.artifactRefs?.length ?? 0) > 0,
    );
  }

  function formatLabelParts(segment: ToolLineageSegment): { name: string; count: string | null } {
    if (segment.count === 1) {
      return { name: segment.displayName, count: null };
    }
    return { name: segment.displayName, count: `×${segment.count}` };
  }
</script>

{#snippet segmentDetail(run: ToolRunState)}
  <div class="space-y-1 text-[11px] leading-relaxed text-surface-400">
    {#if run.inputSummary?.trim()}
      <p class="break-words">
        <span class="text-primary-400/50">in</span>
        {run.inputSummary}
      </p>
    {/if}
    {#if run.outputSummary?.trim()}
      <p class="break-words text-surface-300">
        <span class="text-primary-400/50">out</span>
        {run.outputSummary}
      </p>
    {/if}
    {#if run.artifactRefs && run.artifactRefs.length > 0}
      <p class="text-surface-500">
        {run.artifactRefs.length} receipt{run.artifactRefs.length === 1 ? "" : "s"}
      </p>
    {/if}
    {#if onPromoteToFlow && sessionId && turnIndex && run.status !== "running"}
      <button
        type="button"
        class="workshop-text-action mt-1 inline-flex items-center gap-1 text-[10px]"
        onclick={() =>
          void onPromoteToFlow(
            sliceRefFromChatToolRun({
              sessionId,
              turnIndex,
              runId: run.runId,
              toolRound: run.round,
            }),
          )}
      >
        <Workflow class="h-3 w-3" strokeWidth={2} />
        Save as flow step
      </button>
    {/if}
  </div>
{/snippet}

{#snippet lineageTimeline(segments: ToolLineageSegment[])}
  <ol
    class="relative m-0 list-none space-y-0 p-0 {compact ? 'text-[10px]' : 'text-[11px]'}"
    aria-label="Tool lineage"
  >
    {#each segments as segment, index (segment.key)}
      {@const isLast = index === segments.length - 1}
      {@const detail = segmentHasDetail(segment)}
      {@const parts = formatLabelParts(segment)}
      <li class="relative flex gap-2.5 pb-2 last:pb-0">
        <div class="flex w-3 shrink-0 flex-col items-center pt-1.5">
          <span
            class="block h-1.5 w-1.5 shrink-0 rounded-full {segmentAccentClass(segment.toolName, segment.status)} {segment.status === 'running' ? 'animate-pulse' : ''}"
            aria-hidden="true"
          ></span>
          {#if !isLast}
            <span class="trace-rail mt-0.5 w-px flex-1 min-h-[0.5rem]" aria-hidden="true"></span>
          {/if}
        </div>

        <div class="min-w-0 flex-1">
          {#if detail}
            <details class="group/lineage min-w-0">
              <summary
                class="flex cursor-pointer list-none items-baseline gap-1 marker:content-none transition-colors hover:text-surface-50"
              >
                <span class="tracking-tight {segmentLabelClass(segment.toolName)}">
                  {parts.name}
                </span>
                {#if parts.count}
                  <span class="font-mono text-[10px] text-primary-300/90">{parts.count}</span>
                {/if}
              </summary>
              <div class="mt-1.5 space-y-1.5 border-l border-primary-500/15 pl-2.5">
                {#if segment.count === 1}
                  {@render segmentDetail(segment.runs[0])}
                {:else if segment.count <= 3}
                  {#each segment.runs as run, runIndex (run.runId)}
                    <div>
                      {#if segment.count > 1}
                        <p class="mb-0.5 font-mono text-[10px] text-surface-500">
                          {runIndex + 1}/{segment.count}
                        </p>
                      {/if}
                      {@render segmentDetail(run)}
                    </div>
                  {/each}
                {:else}
                  <div>
                    <p class="mb-0.5 font-mono text-[10px] text-surface-500">1/{segment.count}</p>
                    {@render segmentDetail(segment.runs[0])}
                  </div>
                  <p class="text-surface-500">… {segment.count - 2} more …</p>
                  <div>
                    <p class="mb-0.5 font-mono text-[10px] text-surface-500">
                      {segment.count}/{segment.count}
                    </p>
                    {@render segmentDetail(segment.runs[segment.runs.length - 1])}
                  </div>
                {/if}
              </div>
            </details>
          {:else}
            <p class="flex items-baseline gap-1 tracking-tight">
              <span class={segmentLabelClass(segment.toolName)}>{parts.name}</span>
              {#if parts.count}
                <span class="font-mono text-[10px] text-primary-300/90">{parts.count}</span>
              {/if}
            </p>
          {/if}
        </div>
      </li>
    {/each}
  </ol>
{/snippet}

{#if runs.length > 0}
  {#if inspectorCollapsed}
    <details
      class="tool-trace group/inspector overflow-hidden transition-[border-color,background,box-shadow] duration-200 {footnote
        ? 'chat-tool-footnote'
        : `rounded-lg border ${isDone
          ? 'border-primary-500/20 bg-gradient-to-r from-primary-500/[0.07] via-surface-900/40 to-surface-900/20'
          : 'border-primary-500/30 bg-gradient-to-r from-primary-500/[0.1] via-surface-900/50 to-surface-900/30 shadow-[inset_0_1px_0_rgba(167,139,250,0.08)]'}`}"
      title={fullTrace}
    >
      <summary
        class="flex cursor-pointer list-none items-center gap-2 marker:content-none {footnote
          ? 'py-0.5 text-[10px] text-surface-600 hover:text-surface-400'
          : 'px-2.5 py-1.5'}"
      >
        {#if !footnote}
          <Route
            class="h-3 w-3 shrink-0 text-primary-400/80"
            strokeWidth={2}
            aria-hidden="true"
          />
        {/if}
        <span
          class="min-w-0 flex-1 {footnote
            ? 'font-normal normal-case tracking-normal'
            : 'text-[11px] tabular-nums text-surface-200'}"
        >
          {collapsed.primary}
        </span>
        <ChevronDown
          class="h-3 w-3 shrink-0 text-surface-600 transition-transform duration-200 group-open/inspector:rotate-180"
          strokeWidth={2}
          aria-hidden="true"
        />
      </summary>
      <div
        class="{footnote
          ? 'mt-2 border-t border-surface-700/20 pt-2'
          : 'border-t border-primary-500/10 px-2.5 pb-2 pt-2'}"
      >
        {@render lineageTimeline(lineage)}
      </div>
    </details>
  {:else}
    <div
      class="tool-trace overflow-hidden rounded-lg border border-primary-500/25 bg-gradient-to-br from-primary-500/[0.08] to-surface-900/30 px-2.5 py-2"
    >
      {#if hasRunning && activeSegment}
        <p class="mb-2 flex items-center gap-1.5 text-[11px] text-primary-200">
          <span class="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-primary-400"></span>
          {formatSegmentLabel(activeSegment)}
        </p>
      {/if}
      {@render lineageTimeline(lineage)}
    </div>
  {/if}
{/if}

<style>
  .trace-rail {
    background: linear-gradient(
      to bottom,
      rgb(167 139 250 / 0.45),
      rgb(139 92 246 / 0.2) 55%,
      rgb(52 211 153 / 0.25)
    );
  }

  .tool-trace.group\/inspector[open]:not(.footnote) {
    box-shadow: inset 0 1px 0 rgb(167 139 250 / 0.12);
  }
</style>
