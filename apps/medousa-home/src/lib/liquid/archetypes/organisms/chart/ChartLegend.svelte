<script lang="ts">
  import { chartSeriesColor } from "./chartModel";

  interface Props {
    items: { key: string; label: string }[];
    colors?: string[];
    position?: "top" | "bottom";
  }

  let { items, colors = [], position = "bottom" }: Props = $props();
</script>

{#if items.length}
  <ul
    class="liquid-chart-legend"
    class:liquid-chart-legend-top={position === "top"}
    aria-label="Legend"
  >
    {#each items as item, i (item.key)}
      <li class="liquid-chart-legend-item">
        <span
          class="liquid-chart-legend-swatch"
          style:background={chartSeriesColor(i, colors)}
          aria-hidden="true"
        ></span>
        <span class="liquid-chart-legend-label">{item.label}</span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .liquid-chart-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem 0.85rem;
    margin: 0.4rem 0 0;
    padding: 0;
    list-style: none;
  }

  .liquid-chart-legend-top {
    margin: 0 0 0.4rem;
  }

  .liquid-chart-legend-item {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .liquid-chart-legend-swatch {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 0.15rem;
    flex-shrink: 0;
  }

  .liquid-chart-legend-label {
    font-size: 0.78rem;
    font-weight: 500;
    color: rgb(var(--chart-fg-secondary));
    white-space: nowrap;
  }
</style>
