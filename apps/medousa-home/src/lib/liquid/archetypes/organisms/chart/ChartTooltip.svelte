<script lang="ts">
  /**
   * Glass chart tooltip — paper-readable on light vault, dark glass in chat.
   */
  interface Props {
    visible: boolean;
    x: number;
    y: number;
    title?: string;
    lines?: { label: string; value: string; color?: string }[];
  }

  let { visible, x, y, title = "", lines = [] }: Props = $props();
</script>

{#if visible && (title || lines.length)}
  <div
    class="liquid-chart-tooltip"
    style:left="{x}px"
    style:top="{y}px"
    role="tooltip"
  >
    {#if title}
      <p class="liquid-chart-tooltip-title">{title}</p>
    {/if}
    {#each lines as line, i (i)}
      <p class="liquid-chart-tooltip-line">
        {#if line.color}
          <span
            class="liquid-chart-tooltip-swatch"
            style:background={line.color}
            aria-hidden="true"
          ></span>
        {/if}
        <span class="liquid-chart-tooltip-label">{line.label}</span>
        <span class="liquid-chart-tooltip-value">{line.value}</span>
      </p>
    {/each}
  </div>
{/if}

<style>
  .liquid-chart-tooltip {
    position: absolute;
    z-index: 4;
    pointer-events: none;
    min-width: 6.75rem;
    max-width: 14rem;
    padding: 0.45rem 0.6rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-50) 72%, transparent);
    box-shadow:
      0 10px 28px rgb(0 0 0 / 0.12),
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 55%, transparent) inset;
    backdrop-filter: blur(14px) saturate(1.15);
    -webkit-backdrop-filter: blur(14px) saturate(1.15);
    transform: translate(-50%, calc(-100% - 0.45rem));
    animation: liquid-chart-tip-in 120ms ease-out both;
    color: rgb(var(--color-surface-900));
  }

  :global(html.dark) .liquid-chart-tooltip {
    background: color-mix(in srgb, var(--color-surface-900) 78%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 42%, transparent);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.4);
    color: rgb(var(--color-surface-50));
  }

  .liquid-chart-tooltip-title {
    margin: 0 0 0.22rem;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-900));
  }

  :global(html.dark) .liquid-chart-tooltip-title {
    color: rgb(var(--color-surface-50));
  }

  .liquid-chart-tooltip-line {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin: 0.14rem 0 0;
    font-size: 0.68rem;
    color: rgb(var(--color-surface-600));
  }

  :global(html.dark) .liquid-chart-tooltip-line {
    color: rgb(var(--color-surface-300));
  }

  .liquid-chart-tooltip-swatch {
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 0.12rem;
    flex-shrink: 0;
  }

  .liquid-chart-tooltip-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .liquid-chart-tooltip-value {
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-900));
    font-weight: 700;
  }

  :global(html.dark) .liquid-chart-tooltip-value {
    color: rgb(var(--color-surface-50));
  }

  @keyframes liquid-chart-tip-in {
    from {
      opacity: 0;
      transform: translate(-50%, calc(-100% - 0.2rem));
    }
    to {
      opacity: 1;
      transform: translate(-50%, calc(-100% - 0.45rem));
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-tooltip {
      animation: none;
    }
  }
</style>
