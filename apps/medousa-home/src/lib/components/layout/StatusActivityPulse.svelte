<script lang="ts">
  import ActivityHistoryPopover from "$lib/components/layout/ActivityHistoryPopover.svelte";
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

  /** Lucide audio-lines height ratios (normalized). */
  const AUDIO_BAR_HEIGHTS = [17, 61, 100, 39, 72, 17] as const;

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let nowTick = $state(Date.now());
  let reduceMotion = $state(false);

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

  /** Idle = glyph only; label wakes when something is happening. */
  const showLabel = $derived(hot);
  const displayLabel = $derived(
    showLabel ? truncateActivityLabel(latestLabel, 32) : "",
  );

  $effect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const sync = () => {
      reduceMotion = mq.matches;
    };
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  });

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

<div class="status-activity-pulse" class:status-activity-pulse--idle={!showLabel}>
  <button
    bind:this={triggerEl}
    type="button"
    class="status-activity-pulse-btn"
    class:status-activity-pulse-btn--hot={hot}
    class:status-activity-pulse-btn--open={open}
    class:status-activity-pulse-btn--idle={!showLabel}
    title={latestLabel}
    aria-label="Activity: {latestLabel}"
    aria-expanded={open}
    aria-haspopup="dialog"
    onclick={toggle}
  >
    <span
      class="status-audio-lines"
      class:status-audio-lines--hot={hot && !reduceMotion}
      class:status-audio-lines--lit={hot}
      aria-hidden="true"
    >
      {#each AUDIO_BAR_HEIGHTS as height, index (index)}
        <span
          class="status-audio-bar"
          style="--bar-h: {height}%; --bar-i: {index}"
        ></span>
      {/each}
    </span>
    {#if showLabel}
      <span class="status-activity-pulse-label truncate">{displayLabel}</span>
    {/if}
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
    gap: 0.45rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.05rem 0.25rem;
    margin: 0 -0.25rem;
    color: rgb(var(--color-surface-500));
    font: inherit;
    line-height: 1.2;
    text-align: left;
    transition:
      color 140ms ease,
      background-color 140ms ease;
  }

  .status-activity-pulse-btn--idle {
    opacity: 0.6;
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

  /* Lucide audio-lines silhouette — 6 rounded bars, optically matched to 12px icons. */
  .status-audio-lines {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: 1.5px;
    width: 12px;
    height: 12px;
    line-height: 0;
  }

  .status-audio-bar {
    display: block;
    width: 1.5px;
    height: var(--bar-h);
    min-height: 2px;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.72;
    transform-origin: center center;
    will-change: transform, opacity;
  }

  .status-audio-lines--lit .status-audio-bar {
    opacity: 1;
  }

  /*
    Hot: L→R reveal once, then rolling scaleY wave.
    Wave delay = reveal duration + per-bar stagger so it doesn’t restart mid-reveal.
  */
  .status-audio-lines--hot .status-audio-bar {
    animation:
      status-audio-reveal 0.28s ease-out both,
      status-audio-wave 0.95s ease-in-out infinite;
    animation-delay:
      calc(var(--bar-i) * 70ms),
      calc(0.28s + var(--bar-i) * 70ms);
  }

  @keyframes status-audio-reveal {
    0% {
      opacity: 0;
      transform: scaleY(0.25) scaleX(0.85);
    }
    100% {
      opacity: 1;
      transform: scaleY(1) scaleX(1);
    }
  }

  @keyframes status-audio-wave {
    0%,
    100% {
      transform: scaleY(1) scaleX(1);
    }
    35% {
      transform: scaleY(1.14) scaleX(1.05);
    }
    70% {
      transform: scaleY(0.86) scaleX(0.95);
    }
  }

  .status-activity-pulse-label {
    min-width: 0;
  }
</style>
