<script lang="ts">
  import { PanelRightClose } from "@lucide/svelte";
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import { filterOperatorActivity } from "$lib/utils/activityFilter";
  import { resolveActivityEnrichment } from "$lib/utils/activityEnrichment";
  import { presentActivityEvent } from "$lib/utils/activityPresentation";

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

  const cardsById = $derived(
    new Map(workspace.cards.map((card) => [card.id, card])),
  );

  $effect(() => {
    if (events.length > 0) {
      void workspace.prefetchActivityCardDetails();
    }
  });
</script>

<aside class="flex h-full w-full flex-col" aria-label="Activity and context">
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

  <header class="border-b border-surface-500/45 bg-surface-800/40 px-4 py-3">
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-sm font-semibold tracking-wide text-surface-100">Activity</h2>
      {#if showCollapse && onCollapse}
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-container-token text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-200"
          aria-label="Collapse activity"
          title="Collapse activity"
          onclick={onCollapse}
        >
          <PanelRightClose size={20} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
    {#if error}
      <p class="mt-1 text-xs text-error-400">{error}</p>
    {/if}
  </header>

  <ol class="flex-1 space-y-2 overflow-y-auto p-3">
    {#each [...visibleEvents].reverse() as event (event.id)}
      {@const enrichment = resolveActivityEnrichment(
        event,
        cardsById,
        workspace.cardDetailsCache,
      )}
      {@const item = presentActivityEvent(event, enrichment)}
      <li class="workshop-inset p-3 text-sm">
        <div class="flex items-center justify-between gap-2 text-xs text-surface-300">
          <span class="font-medium uppercase tracking-wide">{item.label}</span>
          <time datetime={event.timestamp_utc}>{item.time}</time>
        </div>
        <p class="mt-1 leading-snug text-surface-50">{item.summary}</p>
        {#if item.context}
          <p class="mt-1 text-xs leading-snug text-surface-400">{item.context}</p>
        {/if}
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
