<script lang="ts">
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkspaceEvent } from "$lib/types/workspace";

  interface Props {
    events: WorkspaceEvent[];
    error: string | null;
    daemonMessage: string | null;
    notePath: string | null;
    noteTitle: string | null;
    wikilinksOut: string[];
    backlinks: string[];
    cardDetail: WorkCardDetail | null;
    cardError: string | null;
    onOpenNote: (path: string) => void;
  }

  let {
    events,
    error,
    daemonMessage,
    notePath,
    noteTitle,
    wikilinksOut,
    backlinks,
    cardDetail,
    cardError,
    onOpenNote,
  }: Props = $props();

  function formatTime(iso: string): string {
    try {
      return new Date(iso).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return iso;
    }
  }
</script>

<aside
  class="flex h-full w-72 shrink-0 flex-col border-l border-surface-500/20 bg-surface-900/60"
  aria-label="Activity and context"
>
  <ContextPanel
    {notePath}
    {noteTitle}
    {wikilinksOut}
    {backlinks}
    {cardDetail}
    {cardError}
    {onOpenNote}
  />

  <header class="border-b border-surface-500/20 px-4 py-3">
    <h2 class="text-sm font-semibold tracking-wide text-surface-200">Activity</h2>
    {#if daemonMessage}
      <p
        class="mt-1 text-xs {daemonMessage.includes('connected')
          ? 'text-success-400'
          : 'text-warning-400'}"
      >
        {daemonMessage}
      </p>
    {/if}
    {#if error}
      <p class="mt-1 text-xs text-error-400">{error}</p>
    {/if}
  </header>

  <ol class="flex-1 space-y-2 overflow-y-auto p-3">
    {#each [...events].reverse() as event (event.id)}
      <li class="rounded-container-token bg-surface-800/70 p-3 text-sm">
        <div class="flex items-center justify-between gap-2 text-xs text-surface-400">
          <span class="capitalize">{event.kind.replaceAll("_", " ")}</span>
          <time datetime={event.timestamp_utc}>{formatTime(event.timestamp_utc)}</time>
        </div>
        <p class="mt-1 text-surface-100">{event.summary}</p>
      </li>
    {:else}
      <li class="px-2 py-6 text-center text-sm text-surface-400">
        Waiting for workspace events…
      </li>
    {/each}
  </ol>
</aside>
