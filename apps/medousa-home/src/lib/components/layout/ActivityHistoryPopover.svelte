<script lang="ts">
  import ActivityStoryFeed from "$lib/components/layout/ActivityStoryFeed.svelte";
  import ActivityToolReceipts from "$lib/components/layout/ActivityToolReceipts.svelte";
  import { activityView } from "$lib/stores/activityView.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { visibleActivityFeed } from "$lib/utils/activityFilter";
  import { buildActivityStory } from "$lib/utils/activityStory";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    triggerEl: HTMLElement | null;
    onClose: () => void;
  }

  let { open, triggerEl, onClose }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  const visibleEvents = $derived(
    visibleActivityFeed(workspace.feed, {
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
    if (visibleEvents.length > 0) {
      workspace.scheduleActivityCardPrefetch();
      activityView.pruneToFeed(new Set(workspace.feed.map((event) => event.id)));
    }
  });

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;

    // Re-place when the story collapses (Clear) or grows — bottom-anchored
    // placement keeps the sheet glued above the status trigger.
    void chapters;
    void visibleEvents.length;
    void activityView.hiddenIds.size;

    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeToolbarPopover(triggerEl, menuEl, {
        prefer: "above",
        width: 360,
        gap: 8,
        pad: 10,
        maxHeightRatio: 0.72,
      });
      frame = window.requestAnimationFrame(() => {
        if (!triggerEl || !menuEl) return;
        placeToolbarPopover(triggerEl, menuEl, {
          prefer: "above",
          width: 360,
          gap: 8,
          pad: 10,
          maxHeightRatio: 0.72,
        });
      });
    };

    void tick().then(place);

    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(place) : null;
    ro?.observe(menuEl);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    return () => {
      window.cancelAnimationFrame(frame);
      ro?.disconnect();
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
    };
  });

  function clearViewed() {
    activityView.clearViewed(visibleEvents.map((event) => event.id));
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="activity-history-scrim"
    role="presentation"
    onclick={onClose}
  ></div>
  <div
    bind:this={menuEl}
    class="activity-history-popover workshop-rail-sheet"
    role="dialog"
    aria-label="Activity history"
  >
    <header class="activity-history-header">
      <div class="min-w-0">
        <h2 class="activity-history-title">Activity</h2>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        {#if visibleEvents.length > 0}
          <button
            type="button"
            class="workshop-text-action text-[11px]"
            onclick={clearViewed}
          >
            Clear
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
      </div>
    </header>

    {#if workspace.streamError}
      <p class="activity-history-error">{workspace.streamError}</p>
    {/if}

    <div class="activity-history-scroll">
      <ActivityStoryFeed
        {chapters}
        emptyHidden={activityView.hiddenIds.size > 0}
        compact
      />
    </div>

    <footer class="activity-history-footer">
      <ActivityToolReceipts sessionScoped={true} limit={2} />
    </footer>
  </div>
{/if}

<style>
  .activity-history-scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
  }

  .activity-history-popover {
    z-index: 71;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    pointer-events: auto;
    padding: 0;
    min-height: 10rem;
    /* Soft settle when Clear collapses the story into the bar. */
    transition: min-height 160ms ease;
  }

  .activity-history-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    flex-shrink: 0;
    padding: 0.7rem 0.85rem 0.55rem;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
  }

  .activity-history-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .activity-history-error {
    margin: 0;
    padding: 0.45rem 0.85rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-error-400));
  }

  .activity-history-scroll {
    min-height: 0;
    flex: 1;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 0.65rem 0.55rem 0.35rem;
    scrollbar-width: thin;
  }

  .activity-history-footer {
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.2);
    padding: 0.35rem 0.55rem 0.45rem;
  }
</style>
