<script lang="ts">
  import ActivityHistoryPopover from "$lib/components/layout/ActivityHistoryPopover.svelte";
  import AudioLinesMark from "$lib/components/ui/AudioLinesMark.svelte";
  import { activityView } from "$lib/stores/activityView.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { visibleActivityFeed } from "$lib/utils/activityFilter";
  import { buildActivityStory } from "$lib/utils/activityStory";
  import {
    isActivityFeedHot,
    truncateActivityLabel,
  } from "$lib/utils/activityPulse";

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let nowTick = $state(Date.now());

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

  const latestBeat = $derived(chapters[0]?.beats[0] ?? null);
  const latestLabel = $derived(latestBeat?.presentation.summary ?? "All quiet");
  const latestAt = $derived(latestBeat?.event.timestamp_utc ?? null);

  const feedHot = $derived(isActivityFeedHot(latestAt, nowTick));
  const hot = $derived(
    feedHot || workshop.runBusy || graphemeScriptEditor.compileBusy,
  );

  /** Always labeled — "Idle" at rest, live summary when something is moving. */
  const displayLabel = $derived(
    hot ? truncateActivityLabel(latestLabel, 32) : "Idle",
  );

  $effect(() => {
    const timer = window.setInterval(() => {
      nowTick = Date.now();
    }, 1000);
    return () => window.clearInterval(timer);
  });

  function toggle() {
    open = !open;
  }

  function close() {
    open = false;
  }
</script>

<div class="status-activity-pulse" class:status-activity-pulse--idle={!hot}>
  <button
    bind:this={triggerEl}
    type="button"
    class="status-activity-pulse-btn"
    class:status-activity-pulse-btn--hot={hot}
    class:status-activity-pulse-btn--open={open}
    class:status-activity-pulse-btn--idle={!hot}
    title={hot ? latestLabel : "Activity — idle"}
    aria-label="Activity: {displayLabel}"
    aria-expanded={open}
    aria-haspopup="dialog"
    onclick={toggle}
  >
    <AudioLinesMark hot={hot} lit={hot} size={13} />
    <span class="status-activity-pulse-label truncate">{displayLabel}</span>
  </button>

  <ActivityHistoryPopover {open} {triggerEl} onClose={close} />
</div>

<style>
  .status-activity-pulse {
    position: relative;
    display: inline-flex;
    align-items: center;
    align-self: center;
    min-width: 0;
    max-width: 16rem;
    flex: 1 1 8rem;
    line-height: 0;
  }

  .status-activity-pulse--idle {
    flex: 0 0 auto;
    max-width: none;
  }

  .status-activity-pulse-btn {
    display: inline-flex;
    max-width: 100%;
    min-width: 0;
    align-items: center;
    gap: 0.5rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.15rem 0.3rem;
    margin: 0;
    color: rgb(var(--color-surface-500));
    font: inherit;
    line-height: 1.2;
    text-align: left;
    transition:
      color 140ms ease,
      background-color 140ms ease;
  }

  .status-activity-pulse-btn--idle {
    opacity: 0.85;
  }

  .status-activity-pulse-btn:hover,
  .status-activity-pulse-btn--open {
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--color-surface-200));
    opacity: 1;
  }

  .status-activity-pulse-btn--hot {
    color: rgb(var(--color-surface-100));
    opacity: 1;
  }

  .status-activity-pulse-label {
    min-width: 0;
  }
</style>
