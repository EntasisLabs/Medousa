<script lang="ts">
  import { PanelRightClose } from "@lucide/svelte";
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
  import ActivityStoryFeed from "$lib/components/layout/ActivityStoryFeed.svelte";
  import { activityView } from "$lib/stores/activityView.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkCardDetail } from "$lib/types/card";
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import { visibleActivityFeed } from "$lib/utils/activityFilter";
  import { buildActivityStory } from "$lib/utils/activityStory";
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

  const chapters = $derived(
    buildActivityStory(visibleEvents, cardsById, workspace.cardDetailsCache),
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

  <header class="activity-story-header">
    <div class="flex min-w-0 items-center justify-between gap-2">
      <div class="min-w-0">
        <h2 class="activity-story-title">Activity</h2>
      </div>
      <div class="activity-story-actions flex shrink-0 items-center gap-2">
        {#if visibleEvents.length > 0}
          <button
            type="button"
            class="workshop-text-action activity-story-action text-[11px]"
            onclick={clearViewed}
          >
            Clear
          </button>
        {/if}
        {#if activityView.hiddenIds.size > 0}
          <button
            type="button"
            class="workshop-text-action activity-story-action text-[11px]"
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
            <PanelRightClose size={18} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
    </div>
    {#if error}
      <p class="mt-2 text-xs text-error-400">{error}</p>
    {/if}
  </header>

  <div class="activity-story-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-3 py-4">
    <ActivityStoryFeed
      {chapters}
      emptyHidden={activityView.hiddenIds.size > 0}
    />
  </div>

  <ActivityToolReceipts sessionScoped={true} limit={3} />
</aside>

<style>
  .activity-story-header {
    flex-shrink: 0;
    padding: 0.9rem 1rem 0.8rem;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
  }

  .activity-story-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .activity-story-action {
    opacity: 0.55;
    transition: opacity 140ms ease;
  }

  .activity-story-header:hover .activity-story-action,
  .activity-story-action:focus-visible {
    opacity: 1;
  }

  .activity-story-scroll {
    scrollbar-width: thin;
    scrollbar-color: rgb(var(--shell-border, var(--color-surface-500)) / 0.45) transparent;
  }

  .activity-story-scroll::-webkit-scrollbar {
    width: 6px;
  }

  .activity-story-scroll::-webkit-scrollbar-thumb {
    border-radius: 999px;
    background: rgb(var(--shell-border, var(--color-surface-500)) / 0.4);
  }

  .activity-story-scroll::-webkit-scrollbar-track {
    background: transparent;
  }
</style>
