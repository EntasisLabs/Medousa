<script lang="ts">
  import { PanelRightClose } from "@lucide/svelte";
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import { filterOperatorActivity } from "$lib/utils/activityFilter";

  interface Props {
    events: WorkspaceEvent[];
    error: string | null;
    notePath: string | null;
    noteTitle: string | null;
    wikilinksOut: string[];
    backlinks: string[];
    cardDetail: WorkCardDetail | null;
    cardError: string | null;
    noteDiffChip: string | null;
    onOpenNote: (path: string) => void;
    showCollapse?: boolean;
    onCollapse?: () => void;
  }

  let {
    events,
    error,
    notePath,
    noteTitle,
    wikilinksOut,
    backlinks,
    cardDetail,
    cardError,
    noteDiffChip,
    onOpenNote,
    showCollapse = false,
    onCollapse,
  }: Props = $props();

  const visibleEvents = $derived(
    filterOperatorActivity(events, {
      showTechnical: settings.showTechnicalActivity,
    }),
  );

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
  class="flex h-full w-full flex-col border-l border-surface-500/20 bg-surface-900/60"
  aria-label="Activity and context"
>
  <ContextPanel
    {notePath}
    {noteTitle}
    {wikilinksOut}
    {backlinks}
    {cardDetail}
    {cardError}
    {noteDiffChip}
    {onOpenNote}
  />

  <header class="border-b border-surface-500/20 px-4 py-3">
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold tracking-wide text-surface-200">Activity</h2>
      {#if showCollapse && onCollapse}
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          aria-label="Collapse activity"
          title="Collapse activity"
          onclick={onCollapse}
        >
          <PanelRightClose size={16} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
    {#if error}
      <p class="mt-1 text-xs text-error-400">{error}</p>
    {/if}
  </header>

  <ol class="flex-1 space-y-2 overflow-y-auto p-3">
    {#each [...visibleEvents].reverse() as event (event.id)}
      <li class="rounded-container-token bg-surface-800/70 p-3 text-sm">
        <div class="flex items-center justify-between gap-2 text-xs text-surface-400">
          <span class="capitalize">{event.kind.replaceAll("_", " ")}</span>
          <time datetime={event.timestamp_utc}>{formatTime(event.timestamp_utc)}</time>
        </div>
        <p class="mt-1 text-surface-100">{event.summary}</p>
      </li>
    {:else}
      <li class="px-2 py-8 text-center">
        <div class="mx-auto mb-3 h-0.5 w-8 rounded-full bg-primary-500"></div>
        <p class="text-sm text-surface-300">All quiet</p>
        <p class="mt-1 text-xs text-surface-500">
          Work and vault updates show up here.
        </p>
      </li>
    {/each}
  </ol>
</aside>
