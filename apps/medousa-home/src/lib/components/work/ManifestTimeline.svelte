<script lang="ts">
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import {
    formatWorkspaceEventKind,
    timelineToolNames,
    timelineVaultPath,
  } from "$lib/utils/cardTimeline";
  import { formatToolName } from "$lib/utils/formatTurn";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    events: WorkspaceEvent[];
    toolNames?: string[] | null;
    onOpenNote?: (path: string) => void;
  }

  let { events, toolNames = null, onOpenNote }: Props = $props();

  let toolsOpen = $state(false);

  function formatTime(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  const allTools = $derived(toolNames?.length ? toolNames : []);
</script>

<section class="manifest-timeline" aria-label="Manifestation timeline">
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-[11px] font-medium uppercase tracking-wide text-surface-500">
      Timeline
    </h2>
    {#if allTools.length > 0}
      <button
        type="button"
        class="workshop-text-action text-[10px]"
        onclick={() => (toolsOpen = !toolsOpen)}
      >
        {allTools.length} tool{allTools.length === 1 ? "" : "s"}
        {toolsOpen ? " ▾" : " ▸"}
      </button>
    {/if}
  </div>

  {#if toolsOpen && allTools.length > 0}
    <p class="mb-3 text-[10px] leading-relaxed text-surface-500">
      {allTools.map((tool) => formatToolName(tool)).join(" · ")}
    </p>
  {/if}

  {#if events.length === 0}
    <p class="text-sm text-surface-500">No events yet — work is just starting.</p>
  {:else}
    <ol class="manifest-timeline-list">
      {#each events as event, index (event.id)}
        {@const vaultPath = timelineVaultPath(event)}
        {@const eventTools = timelineToolNames(event)}
        {@const isLast = index === events.length - 1}
        <li class="manifest-timeline-item {isLast ? 'manifest-timeline-item-last' : ''}">
          <span class="manifest-timeline-dot" aria-hidden="true"></span>
          <div class="min-w-0 flex-1 pb-4">
            <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
              <span class="text-[10px] font-medium uppercase tracking-wide text-primary-300/80">
                {formatWorkspaceEventKind(event.kind)}
              </span>
              <span class="text-[10px] text-surface-600">{formatTime(event.timestamp_utc)}</span>
            </div>
            <p class="mt-1 text-sm leading-relaxed text-surface-200">{event.summary}</p>
            {#if event.detail_line?.trim()}
              <p class="mt-1 text-[11px] text-surface-500">{event.detail_line}</p>
            {/if}
            {#if vaultPath && onOpenNote}
              <button
                type="button"
                class="workshop-text-action mt-1.5 text-[11px]"
                onclick={() => onOpenNote(vaultPath)}
              >
                Open {vaultDisplayTitle(vaultPath, vaultPath)}
              </button>
            {/if}
            {#if eventTools.length > 0}
              <p class="mt-1 font-mono text-[10px] text-surface-600">
                {eventTools.map((tool) => formatToolName(tool)).join(" · ")}
              </p>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}
</section>
