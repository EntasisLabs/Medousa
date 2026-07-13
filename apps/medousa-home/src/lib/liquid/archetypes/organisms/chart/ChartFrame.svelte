<script lang="ts">
  import { LayerCake, Svg, Html } from "layercake";
  import { scaleBand, scaleLinear } from "d3-scale";
  import type { Snippet } from "svelte";
  import type { ChartViewModel } from "./chartModel";
  import { toCartesianRows, yMax } from "./chartModel";
  import ChartTooltip from "./ChartTooltip.svelte";

  interface Props {
    model: ChartViewModel;
    children: Snippet;
  }

  let { model, children }: Props = $props();

  const rows = $derived(toCartesianRows(model));
  const maxY = $derived(yMax(model));
  const horizontal = $derived(model.layout === "horizontal" && model.type === "bar");

  let tipVisible = $state(false);
  let tipX = $state(0);
  let tipY = $state(0);
  let tipTitle = $state("");
  let tipLines = $state<{ label: string; value: string; color?: string }[]>([]);

  function showTooltip(
    localX: number,
    localY: number,
    title: string,
    lines: { label: string; value: string; color?: string }[],
  ) {
    if (!model.tooltip) return;
    tipVisible = true;
    tipX = localX;
    tipY = localY;
    tipTitle = title;
    tipLines = lines;
  }

  function hideTooltip() {
    tipVisible = false;
  }

  const cakeCustom = $derived({
    series: model.series,
    seriesMarks: model.seriesMarks,
    colors: model.colors,
    stacked: model.stacked,
    curve: model.curve,
    labels: model.labels,
    labelPosition: model.labelPosition,
    activeKey: model.activeKey,
    interactive: model.interactive,
    separator: model.separator,
    horizontal,
    chartType: model.type,
    showTooltip,
    hideTooltip,
  });
</script>

<div class="liquid-chart-frame">
  {#if horizontal}
    <LayerCake
      data={rows}
      xScale={scaleLinear()}
      y="category"
      yScale={scaleBand().paddingInner(0.4).paddingOuter(0.18)}
      xDomain={[0, maxY * 1.08]}
      yDomain={model.categories}
      padding={{ top: 8, right: 16, bottom: 24, left: 56 }}
      custom={cakeCustom}
    >
      <Svg>{@render children()}</Svg>
      {#if model.tooltip}
        <Html>
          <ChartTooltip
            visible={tipVisible}
            x={tipX}
            y={tipY}
            title={tipTitle}
            lines={tipLines}
          />
        </Html>
      {/if}
    </LayerCake>
  {:else}
    <LayerCake
      data={rows}
      x="category"
      xScale={scaleBand().paddingInner(0.4).paddingOuter(0.18)}
      yScale={scaleLinear()}
      xDomain={model.categories}
      yDomain={[0, maxY * 1.08]}
      padding={{ top: 10, right: 12, bottom: 28, left: 36 }}
      custom={cakeCustom}
    >
      <Svg>{@render children()}</Svg>
      {#if model.tooltip}
        <Html>
          <ChartTooltip
            visible={tipVisible}
            x={tipX}
            y={tipY}
            title={tipTitle}
            lines={tipLines}
          />
        </Html>
      {/if}
    </LayerCake>
  {/if}
</div>

<style>
  .liquid-chart-frame {
    position: relative;
    width: 100%;
    height: var(--liquid-chart-height, 14.5rem);
    min-height: 10rem;
  }

  .liquid-chart-frame :global(.layercake-container) {
    width: 100%;
    height: 100%;
  }
</style>
