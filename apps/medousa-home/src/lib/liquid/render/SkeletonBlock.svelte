<script lang="ts">
  /** Generic bones for a node in `skeleton` state — the instant-paint placeholder. */
  interface Props {
    type?: string;
    lines?: number;
  }
  let { type = "", lines = 3 }: Props = $props();

  const count = $derived(type === "media" || type === "image" ? 1 : lines);
  const block = $derived(type === "media" || type === "image");
</script>

<div class="liquid-skeleton" data-type={type} aria-hidden="true">
  {#if block}
    <div class="liquid-skeleton-media"></div>
  {:else}
    {#each Array.from({ length: count }) as _, index (index)}
      <div class="liquid-skeleton-line" style={`width:${index === count - 1 ? 60 : 100 - index * 8}%`}></div>
    {/each}
  {/if}
</div>

<style>
  .liquid-skeleton {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
  }

  .liquid-skeleton-line {
    height: 0.75rem;
    border-radius: 0.375rem;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-surface-700) 55%, transparent) 25%,
      color-mix(in srgb, var(--color-surface-500) 55%, transparent) 37%,
      color-mix(in srgb, var(--color-surface-700) 55%, transparent) 63%
    );
    background-size: 400% 100%;
    animation: liquid-shimmer 1.4s ease infinite;
  }

  .liquid-skeleton-media {
    aspect-ratio: 16 / 9;
    width: 100%;
    border-radius: 0.75rem;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-surface-700) 55%, transparent) 25%,
      color-mix(in srgb, var(--color-surface-500) 55%, transparent) 37%,
      color-mix(in srgb, var(--color-surface-700) 55%, transparent) 63%
    );
    background-size: 400% 100%;
    animation: liquid-shimmer 1.4s ease infinite;
  }

  @keyframes liquid-shimmer {
    0% {
      background-position: 100% 0;
    }
    100% {
      background-position: 0 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-skeleton-line,
    .liquid-skeleton-media {
      animation: none;
    }
  }
</style>
