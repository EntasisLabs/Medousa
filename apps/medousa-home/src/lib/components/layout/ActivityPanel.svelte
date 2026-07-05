<script lang="ts">
  import { PanelRightClose } from "@lucide/svelte";
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
  import { activityView } from "$lib/stores/activityView.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import { visibleActivityFeed } from "$lib/utils/activityFilter";
  import { resolveActivityEnrichment } from "$lib/utils/activityEnrichment";
  import { presentActivityEvent } from "$lib/utils/activityPresentation";
  import ActivityToolReceipts from "$lib/components/layout/ActivityToolReceipts.svelte";

  interface Props {
    events: WorkspaceEvent[];
    error: string | null;
    notePath: string | null;
    noteTitle: string | null;
    wikilinksOut: string[];
    backlinks: string[];
    browserUrl?: string | null;
    browserTitle?: string | null;
    cardDetail: WorkCardDetail | null;
    cardError: string | null;
    noteDiffChip: string | null;
    onOpenNote: (path: string) => void;
    onOpenWeb?: () => void;
    onSelectCard?: (id: string) => void;
    onCollapse?: () => void;
  }

  let {
    events,
    error,
    notePath,
    noteTitle,
    wikilinksOut,
    backlinks,
    browserUrl = null,
    browserTitle = null,
    cardDetail,
    cardError,
    noteDiffChip,
    onOpenNote,
    onOpenWeb,
    onSelectCard,
    onCollapse,
  }: Props = $props();

  const visibleEvents = $derived(
    visibleActivityFeed(events, {
      showTechnical: settings.showTechnicalActivity,
      hiddenIds: activityView.hiddenIds,
    }),
  );

  const cardsById = $derived(
    new Map(workspace.cards.map((card) => [card.id, card])),
  );

  $effect(() => {
    if (events.length > 0) {
      workspace.scheduleActivityCardPrefetch();
      activityView.pruneToFeed(new Set(events.map((event) => event.id)));
    }
  });

  function clearViewed() {
    activityView.clearViewed(visibleEvents.map((event) => event.id));
  }
</script>

<aside
  class="activity-panel flex h-full min-w-0 w-full flex-col overflow-hidden"
  aria-label="Activity and context"
>
  <ContextPanel
    {notePath}
    {noteTitle}
    {wikilinksOut}
    {backlinks}
    {browserUrl}
    {browserTitle}
    {cardDetail}
    {cardError}
    {noteDiffChip}
    {onOpenNote}
    {onOpenWeb}
    {onSelectCard}
  />

  <header class="shrink-0 border-b border-surface-500/45 bg-surface-800/40 px-4 py-3">
    <div class="flex min-w-0 items-center justify-between gap-2">
      <h2 class="min-w-0 truncate text-sm font-semibold tracking-wide text-surface-100">
        Activity
      </h2>
      <div class="flex shrink-0 items-center gap-2">
        {#if visibleEvents.length > 0}
          <button
            type="button"
            class="workshop-text-action text-[11px]"
            onclick={clearViewed}
          >
            Clear viewed
          </button>
        {/if}
        {#if activityView.hiddenIds.size > 0}
          <button
            type="button"
            class="workshop-text-action text-[11px]"
            onclick={() => activityView.restoreAll()}
          >
            Show all
          </button>
        {/if}
        {#if onCollapse}
          <button
            type="button"
            class="workshop-rail-btn shrink-0"
            aria-label="Collapse activity panel"
            title="Collapse activity panel"
            onclick={onCollapse}
          >
            <PanelRightClose size={20} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
    </div>
    {#if error}
      <p class="mt-1 text-xs text-error-400">{error}</p>
    {/if}
  </header>

  <ol class="min-h-0 flex-1 space-y-2 overflow-y-auto overflow-x-hidden p-3">
    {#each [...visibleEvents].reverse() as event (event.id)}
      {@const enrichment = resolveActivityEnrichment(
        event,
        cardsById,
        workspace.cardDetailsCache,
      )}
      {@const item = presentActivityEvent(event, enrichment)}
      <li class="workshop-inset min-w-0 p-3 text-sm">
        <div class="flex min-w-0 items-center justify-between gap-2 text-xs text-surface-300">
          <span class="min-w-0 truncate font-medium uppercase tracking-wide">{item.label}</span>
          <time class="shrink-0 tabular-nums" datetime={event.timestamp_utc}>{item.time}</time>
        </div>
        <p class="mt-1 break-words leading-snug text-surface-50">{item.summary}</p>
        {#if item.context}
          <p class="mt-1 break-words text-xs leading-snug text-surface-400">{item.context}</p>
        {/if}
      </li>
    {:else}
      <li class="px-2 py-8 text-center">
        <div class="mx-auto mb-3 h-0.5 w-8 rounded-full bg-primary-500"></div>
        <p class="text-sm text-surface-300">All quiet</p>
        <p class="mt-1 text-xs text-surface-500">
          {#if activityView.hiddenIds.size > 0}
            Cleared from view on this device. New updates still appear here.
          {:else}
            Work and vault updates show up here.
          {/if}
        </p>
      </li>
    {/each}
  </ol>

  <ActivityToolReceipts sessionScoped={true} />
</aside>
