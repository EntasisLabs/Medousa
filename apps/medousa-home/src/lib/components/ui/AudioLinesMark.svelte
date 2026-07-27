<script lang="ts">
  /** Lucide audio-lines height ratios (normalized). */
  const AUDIO_BAR_HEIGHTS = [17, 61, 100, 39, 72, 17] as const;

  interface Props {
    /** Run reveal + wave animation. */
    hot?: boolean;
    /** Full bar opacity (busy / lit). */
    lit?: boolean;
    /** Icon box size in px (status bar uses 13). */
    size?: number;
  }

  let { hot = false, lit = false, size = 13 }: Props = $props();

  let reduceMotion = $state(false);

  const animateHot = $derived(hot && !reduceMotion);

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
</script>

<span
  class="audio-lines-mark"
  class:audio-lines-mark--hot={animateHot}
  class:audio-lines-mark--lit={lit}
  style="--audio-lines-size: {size}px"
  aria-hidden="true"
>
  {#each AUDIO_BAR_HEIGHTS as height, index (index)}
    <span
      class="audio-lines-mark-bar"
      style="--bar-h: {height}%; --bar-i: {index}"
    ></span>
  {/each}
</span>

<style>
  /* Lucide audio-lines silhouette — 6 rounded bars. */
  .audio-lines-mark {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: 1.5px;
    width: var(--audio-lines-size, 13px);
    height: var(--audio-lines-size, 13px);
    line-height: 0;
    color: inherit;
  }

  .audio-lines-mark-bar {
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

  .audio-lines-mark--lit .audio-lines-mark-bar {
    opacity: 1;
  }

  /*
    Hot: L→R reveal once, then rolling scaleY wave.
    Wave delay = reveal duration + per-bar stagger so it doesn’t restart mid-reveal.
  */
  .audio-lines-mark--hot .audio-lines-mark-bar {
    animation:
      audio-lines-reveal 0.28s ease-out both,
      audio-lines-wave 0.95s ease-in-out infinite;
    animation-delay:
      calc(var(--bar-i) * 70ms),
      calc(0.28s + var(--bar-i) * 70ms);
  }

  @keyframes audio-lines-reveal {
    0% {
      opacity: 0;
      transform: scaleY(0.25) scaleX(0.85);
    }
    100% {
      opacity: 1;
      transform: scaleY(1) scaleX(1);
    }
  }

  @keyframes audio-lines-wave {
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
</style>
