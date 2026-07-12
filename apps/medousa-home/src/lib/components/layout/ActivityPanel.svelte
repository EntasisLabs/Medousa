<script lang="ts">
  import { PanelRightClose } from "@lucide/svelte";
  import ContextPanel from "$lib/components/layout/ContextPanel.svelte";
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
    {#if chapters.length === 0}
      <div class="px-2 py-12 text-center">
        <p class="text-sm text-surface-300">All quiet</p>
        <p class="mt-1.5 text-xs leading-relaxed text-surface-500">
          {#if activityView.hiddenIds.size > 0}
            Cleared on this device — new work still lands here.
          {:else}
            Saves and work gather here as a short story.
          {/if}
        </p>
      </div>
    {:else}
      {#each chapters as chapter (chapter.key)}
        <section class="activity-story-chapter">
          <h3 class="activity-story-chapter-label">{chapter.label}</h3>
          <ol class="activity-story-list">
            {#each chapter.beats as beat (beat.id)}
              <li class="activity-story-beat">
                {#if beat.kicker}
                  <p class="activity-story-beat-kicker">{beat.kicker}</p>
                {/if}
                <div class="activity-story-beat-row">
                  <p class="activity-story-beat-summary">{beat.presentation.summary}</p>
                  <time
                    class="activity-story-beat-time"
                    datetime={beat.event.timestamp_utc}
                  >
                    {beat.presentation.time}
                  </time>
                </div>
                {#if beat.presentation.context}
                  <p class="activity-story-beat-context">{beat.presentation.context}</p>
                {/if}
              </li>
            {/each}
          </ol>
        </section>
      {/each}
    {/if}
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

  .activity-story-chapter {
    margin-bottom: 1.45rem;
  }

  .activity-story-chapter-label {
    margin: 0 0 0.55rem;
    padding: 0 0.4rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.9);
  }

  .activity-story-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .activity-story-beat {
    padding: 0.7rem 0.45rem;
    border-radius: 0.55rem;
    transition: background 140ms ease;
  }

  .activity-story-beat + .activity-story-beat {
    margin-top: 0.15rem;
  }

  .activity-story-beat:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.28);
  }

  .activity-story-beat-kicker {
    margin: 0 0 0.2rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .activity-story-beat-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.85rem;
  }

  .activity-story-beat-time {
    flex-shrink: 0;
    font-size: 0.625rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.85);
  }

  .activity-story-beat-summary {
    margin: 0;
    min-width: 0;
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.4;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
    overflow-wrap: anywhere;
  }

  .activity-story-beat-context {
    margin: 0.25rem 0 0;
    font-size: 0.6875rem;
    line-height: 1.4;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.92);
    overflow-wrap: anywhere;
  }
</style>
