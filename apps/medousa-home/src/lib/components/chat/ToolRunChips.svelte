<script lang="ts">
  import { ChevronRight, Workflow } from "@lucide/svelte";
  import ToolActivitySheet from "$lib/components/chat/ToolActivitySheet.svelte";
  import { haptic } from "$lib/haptics";
  import type { ToolRunState } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import { sliceRefFromChatToolRun } from "$lib/types/toolHistory";
  import { toolActivitySummary } from "$lib/utils/toolActivitySummary";
  import type { ToolLineageSegment } from "$lib/utils/toolRunLineage";
  import {
    buildToolLineage,
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
  const summary = $derived(toolActivitySummary(runs));
  const fullTrace = $derived(formatLineagePreview(lineage));
  const hasRunning = $derived(summary.running > 0);
  const activeSegment = $derived(
    hasRunning ? lineage.find((segment) => segment.status === "running") : null,
  );
  let activityOpen = $state(false);

  function openActivity() {
    haptic("light");
    activityOpen = true;
  }

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
  <div class="space-y-1 text-[11px] leading-relaxed text-content-tertiary">
    {#if run.inputParams && run.inputParams.length > 0}
      <dl class="m-0 space-y-0.5">
        {#each run.inputParams as param (param.key)}
          <div class="flex gap-1.5">
            <dt class="shrink-0 font-mono text-[10px] text-primary-400/60">{param.key}</dt>
            <dd class="m-0 min-w-0 flex-1 break-words text-content-secondary">
              {param.value}{#if param.truncated}<span class="text-content-faint">…</span>{/if}
            </dd>
          </div>
        {/each}
      </dl>
    {:else if run.inputSummary?.trim()}
      <p class="break-words">
        <span class="text-primary-400/50">in</span>
        {run.inputSummary}
      </p>
    {/if}
    {#if run.outputSummary?.trim()}
      <p class="break-words text-content-secondary">
        <span class="text-primary-400/50">out</span>
        {run.outputSummary}
      </p>
    {/if}
    {#if run.artifactRefs && run.artifactRefs.length > 0}
      <p class="text-content-quiet">
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
                  <span class="font-mono text-[10px] text-content-link/90">{parts.count}</span>
                {/if}
              </summary>
              <div class="mt-1.5 space-y-1.5 border-l border-primary-500/15 pl-2.5">
                {#if segment.count === 1}
                  {@render segmentDetail(segment.runs[0])}
                {:else if segment.count <= 3}
                  {#each segment.runs as run, runIndex (run.runId)}
                    <div>
                      {#if segment.count > 1}
                        <p class="mb-0.5 font-mono text-[10px] text-content-quiet">
                          {runIndex + 1}/{segment.count}
                        </p>
                      {/if}
                      {@render segmentDetail(run)}
                    </div>
                  {/each}
                {:else}
                  <div>
                    <p class="mb-0.5 font-mono text-[10px] text-content-quiet">1/{segment.count}</p>
                    {@render segmentDetail(segment.runs[0])}
                  </div>
                  <p class="text-content-quiet">… {segment.count - 2} more …</p>
                  <div>
                    <p class="mb-0.5 font-mono text-[10px] text-content-quiet">
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
                <span class="font-mono text-[10px] text-content-link/90">{parts.count}</span>
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
    <button
      type="button"
      class="tool-trace tool-context-trigger"
      title={fullTrace}
      aria-haspopup="dialog"
      aria-expanded={activityOpen}
      onclick={openActivity}
    >
      <Workflow class="tool-context-icon" size={13} aria-hidden="true" />
      <span class="tool-context-count">{summary.countLabel}</span>
      {#if summary.context}
        <span class="tool-context-preview">
          {#if hasRunning}<span class="tool-context-running" aria-hidden="true"></span>{/if}
          <span class="tool-context-label">{summary.context}</span>
        </span>
      {/if}
      {#if summary.failed > 0}
        <span class="tool-context-failed">{summary.failed} failed</span>
      {/if}
      <ChevronRight class="tool-context-chevron" size={13} aria-hidden="true" />
    </button>
    <ToolActivitySheet
      open={activityOpen}
      {runs}
      {sessionId}
      {turnIndex}
      {onPromoteToFlow}
      onClose={() => (activityOpen = false)}
    />
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

  .tool-context-trigger {
    display: grid;
    grid-template-columns: 13px auto minmax(0, 1fr) auto 13px;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    min-width: 0;
    min-height: 30px;
    padding: 0.25rem 0.5rem;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    font-size: 12px;
    color: rgb(var(--theme-text-tertiary));
    transition: background-color 150ms;
  }

  .tool-context-trigger:hover {
    background: rgb(var(--theme-card-hover) / 0.6);
  }

  .tool-context-count {
    grid-column: 2;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    color: rgb(var(--theme-text-secondary));
  }

  .tool-context-preview {
    grid-column: 3;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .tool-context-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-context-running {
    width: 5px;
    height: 5px;
    flex-shrink: 0;
    border-radius: 50%;
    background: rgb(var(--theme-text-tertiary));
  }

  .tool-context-failed {
    grid-column: 4;
    border-left: 1px solid rgb(var(--theme-border));
    padding-left: 0.5rem;
    color: rgb(var(--theme-warning));
    white-space: nowrap;
  }

  .tool-context-trigger :global(.tool-context-icon) {
    grid-column: 1;
  }

  .tool-context-trigger :global(.tool-context-chevron) {
    grid-column: 5;
  }

  @media (max-width: 480px) {
    .tool-context-preview {
      grid-row: 2;
      grid-column: 2 / 5;
    }
    .tool-context-count,
    .tool-context-failed,
    .tool-context-trigger :global(svg) {
      grid-row: 1;
    }
  }

  @media (pointer: coarse) {
    .tool-context-trigger { min-height: 44px; }
  }

  @media (prefers-reduced-motion: reduce) {
    .tool-context-trigger { transition: none; }
  }
</style>
