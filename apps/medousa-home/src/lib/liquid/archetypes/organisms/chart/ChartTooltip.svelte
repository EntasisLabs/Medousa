<script lang="ts">
  /**
   * Minimal chart tooltip shell — values only in v1.
   * Shape accepts richer content later (multi-series rows, icons, etc.).
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
    transform: translate(-50%, calc(-100% - 0.45rem));
    min-width: 6.5rem;
    max-width: 14rem;
    padding: 0.4rem 0.55rem;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 92%, transparent);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.28);
    backdrop-filter: blur(8px);
  }

  .liquid-chart-tooltip-title {
    margin: 0 0 0.2rem;
    font-size: 0.7rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .liquid-chart-tooltip-line {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    margin: 0.12rem 0 0;
    font-size: 0.68rem;
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
    color: rgb(var(--color-surface-100));
    font-weight: 600;
  }
</style>
