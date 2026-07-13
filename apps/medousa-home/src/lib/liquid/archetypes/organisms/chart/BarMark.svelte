<script lang="ts">
  import { getContext } from "svelte";
  import type {
    LiquidChartLabelPosition,
    LiquidChartLabels,
    LiquidChartSeries,
  } from "$lib/markdown/liquidEmbeds";
  import {
    chartSeriesColor,
    formatChartLabel,
    formatChartNumber,
    hasActiveHighlight,
    isActiveKey,
    resolveLabelPosition,
  } from "./chartModel";

  type Scale = ((v: unknown) => number) & {
    bandwidth?: () => number;
    domain?: () => unknown[];
  };

  interface CakeCustom {
    series: LiquidChartSeries[];
    colors: string[];
    stacked: boolean;
    horizontal: boolean;
    labels: LiquidChartLabels;
    labelPosition: LiquidChartLabelPosition;
    activeKey: string;
    chartType: string;
    showTooltip: (
      x: number,
      y: number,
      title: string,
      lines: { label: string; value: string; color?: string }[],
    ) => void;
    hideTooltip: () => void;
  }

  const { data, xScale, yScale, width, height, custom } = getContext<{
    data: import("svelte/store").Readable<Record<string, string | number>[]>;
    xScale: import("svelte/store").Readable<Scale>;
    yScale: import("svelte/store").Readable<Scale>;
    width: import("svelte/store").Readable<number>;
    height: import("svelte/store").Readable<number>;
    custom: import("svelte/store").Readable<CakeCustom>;
  }>("LayerCake");

  interface BarRect {
    key: string;
    seriesKey: string;
    x: number;
    y: number;
    width: number;
    height: number;
    color: string;
    category: string;
    label: string;
    value: number;
    active: boolean;
  }

  interface ValueLabel {
    key: string;
    x: number;
    y: number;
    text: string;
    anchor: "start" | "middle" | "end";
  }

  interface HitBand {
    category: string;
    x: number;
    y: number;
    width: number;
    height: number;
  }

  const bars = $derived.by((): BarRect[] => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    const h = $height;
    if (!cfg || !rows.length || !xS || !yS) return [];
    const out: BarRect[] = [];
    const series = cfg.series;
    const n = Math.max(series.length, 1);
    const highlight = hasActiveHighlight(cfg.activeKey);

    for (const row of rows) {
      const category = String(row.category ?? "");
      if (cfg.horizontal) {
        const y = yS(category) ?? 0;
        const band = yS.bandwidth?.() ?? 12;
        if (cfg.stacked) {
          let cursor = 0;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const x0 = xS(cursor) ?? 0;
            const x1 = xS(cursor + value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              seriesKey: s.key,
              x: Math.min(x0, x1),
              y,
              width: Math.abs(x1 - x0),
              height: band,
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
              active: !highlight || isActiveKey(cfg.activeKey, { key: s.key, label: s.label, category }),
            });
            cursor += value;
          });
        } else {
          const slot = band / n;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const x0 = xS(0) ?? 0;
            const x1 = xS(value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              seriesKey: s.key,
              x: Math.min(x0, x1),
              y: y + si * slot,
              width: Math.abs(x1 - x0),
              height: Math.max(slot * 0.9, 2),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
              active: !highlight || isActiveKey(cfg.activeKey, { key: s.key, label: s.label, category }),
            });
          });
        }
      } else {
        const x = xS(category) ?? 0;
        const band = xS.bandwidth?.() ?? 12;
        if (cfg.stacked) {
          let cursor = 0;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const y0 = yS(cursor) ?? 0;
            const y1 = yS(cursor + value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              seriesKey: s.key,
              x,
              y: Math.min(y0, y1),
              width: band,
              height: Math.abs(y1 - y0),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
              active: !highlight || isActiveKey(cfg.activeKey, { key: s.key, label: s.label, category }),
            });
            cursor += value;
          });
        } else {
          const slot = band / n;
          series.forEach((s, si) => {
            const value = Number(row[s.key] ?? 0);
            const y0 = yS(0) ?? h;
            const y1 = yS(value) ?? 0;
            out.push({
              key: `${category}-${s.key}`,
              seriesKey: s.key,
              x: x + si * slot,
              y: Math.min(y0, y1),
              width: Math.max(slot * 0.9, 2),
              height: Math.abs(y1 - y0),
              color: chartSeriesColor(si, cfg.colors),
              category,
              label: s.label,
              value,
              active: !highlight || isActiveKey(cfg.activeKey, { key: s.key, label: s.label, category }),
            });
          });
        }
      }
    }
    return out;
  });

  const valueLabels = $derived.by((): ValueLabel[] => {
    const cfg = $custom;
    const xS = $xScale;
    const yS = $yScale;
    const rows = $data ?? [];
    if (!cfg || !xS || !yS) return [];
    const pos = resolveLabelPosition({
      type: "bar",
      labels: cfg.labels,
      labelPosition: cfg.labelPosition,
      centerLabel: "",
      centerValue: "",
    });
    if (pos === "none") return [];

    const out: ValueLabel[] = [];
    if (cfg.stacked) {
      for (const row of rows) {
        const category = String(row.category ?? "");
        let total = 0;
        for (const s of cfg.series) total += Number(row[s.key] ?? 0);
        const text = formatChartLabel(cfg.labels, category, total);
        if (!text) continue;
        if (cfg.horizontal) {
          const y = (yS(category) ?? 0) + ((yS.bandwidth?.() ?? 0) / 2);
          const x = xS(total) ?? 0;
          out.push({ key: `lbl-${category}`, x: x + 4, y, text, anchor: "start" });
        } else {
          const x = (xS(category) ?? 0) + ((xS.bandwidth?.() ?? 0) / 2);
          const y = (yS(total) ?? 0) - 4;
          out.push({ key: `lbl-${category}`, x, y, text, anchor: "middle" });
        }
      }
      return out;
    }

    for (const bar of bars) {
      const text = formatChartLabel(cfg.labels, bar.category, bar.value);
      if (!text) continue;
      if (cfg.horizontal) {
        out.push({
          key: `lbl-${bar.key}`,
          x: bar.x + bar.width + 4,
          y: bar.y + bar.height / 2,
          text,
          anchor: "start",
        });
      } else {
        out.push({
          key: `lbl-${bar.key}`,
          x: bar.x + bar.width / 2,
          y: bar.y - 4,
          text,
          anchor: "middle",
        });
      }
    }
    return out;
  });

  const hitBands = $derived.by((): HitBand[] => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    const w = $width;
    const h = $height;
    if (!cfg || !rows.length || !xS || !yS) return [];
    return rows.map((row) => {
      const category = String(row.category ?? "");
      if (cfg.horizontal) {
        return {
          category,
          x: 0,
          y: yS(category) ?? 0,
          width: w,
          height: yS.bandwidth?.() ?? 12,
        };
      }
      return {
        category,
        x: xS(category) ?? 0,
        y: 0,
        width: xS.bandwidth?.() ?? 12,
        height: h,
      };
    });
  });

  const axis = $derived.by((): {
    w: number;
    h: number;
    grid: { x?: number; y?: number }[];
    xLabels: { label: string; x: number }[];
    yLabels: { label: string; y: number }[];
  } => {
    const rows = $data ?? [];
    const xS = $xScale;
    const yS = $yScale;
    const cfg = $custom;
    const w = $width;
    const h = $height;
    if (!cfg || !xS || !yS) {
      return { grid: [], xLabels: [], yLabels: [], w: 0, h: 0 };
    }
    if (cfg.horizontal) {
      const domain = (xS.domain?.() as number[]) ?? [0, 1];
      const max = Number(domain[1] ?? 1);
      return {
        w,
        h,
        grid: [0.25, 0.5, 0.75, 1].map((t) => ({ x: xS(max * t) ?? 0 })),
        xLabels: [0, 0.5, 1].map((t) => ({
          label: String(Math.round(max * t)),
          x: xS(max * t) ?? 0,
        })),
        yLabels: rows.map((row) => {
          const category = String(row.category ?? "");
          return {
            label: category,
            y: (yS(category) ?? 0) + ((yS.bandwidth?.() ?? 0) / 2),
          };
        }),
      };
    }
    const domain = (yS.domain?.() as number[]) ?? [0, 1];
    const max = Number(domain[1] ?? 1);
    return {
      w,
      h,
      grid: [0.25, 0.5, 0.75, 1].map((t) => ({ y: yS(max * t) ?? 0 })),
      xLabels: rows.map((row) => {
        const category = String(row.category ?? "");
        return {
          label: category,
          x: (xS(category) ?? 0) + ((xS.bandwidth?.() ?? 0) / 2),
        };
      }),
      yLabels: [0, 0.5, 1].map((t) => ({
        label: String(Math.round(max * t)),
        y: yS(max * t) ?? 0,
      })),
    };
  });

  function tipLinesForCategory(category: string) {
    const cfg = $custom;
    const rows = $data ?? [];
    if (!cfg) return [];
    const row = rows.find((r) => String(r.category ?? "") === category);
    if (!row) return [];
    return cfg.series.map((s, si) => ({
      label: s.label,
      value: formatChartNumber(Number(row[s.key] ?? 0)),
      color: chartSeriesColor(si, cfg.colors),
    }));
  }

  function onBandEnter(band: HitBand, event: MouseEvent) {
    const cfg = $custom;
    if (!cfg) return;
    const target = event.currentTarget as SVGRectElement;
    const box = target.getBoundingClientRect();
    const parent = target.ownerSVGElement?.parentElement?.getBoundingClientRect();
    if (!parent) return;
    cfg.showTooltip(
      box.left - parent.left + box.width / 2,
      box.top - parent.top + 8,
      band.category,
      tipLinesForCategory(band.category),
    );
  }
</script>

{#if axis.w > 0 && axis.h > 0}
  <g class="liquid-chart-bars">
    {#if $custom?.horizontal}
      {#each axis.grid as g, i (i)}
        <line class="liquid-chart-grid" x1={g.x ?? 0} x2={g.x ?? 0} y1="0" y2={axis.h} />
      {/each}
    {:else}
      {#each axis.grid as g, i (i)}
        <line class="liquid-chart-grid" x1="0" x2={axis.w} y1={g.y ?? 0} y2={g.y ?? 0} />
      {/each}
    {/if}

    {#each axis.yLabels as tick (tick.label + tick.y)}
      <text class="liquid-chart-axis" x={-6} y={tick.y} text-anchor="end" dominant-baseline="middle"
        >{tick.label}</text
      >
    {/each}
    {#each axis.xLabels as tick (tick.label + tick.x)}
      <text class="liquid-chart-axis" x={tick.x} y={axis.h + 14} text-anchor="middle">{tick.label}</text>
    {/each}

    {#each hitBands as band (band.category)}
      <rect
        class="liquid-chart-hit"
        role="img"
        aria-label={band.category}
        x={band.x}
        y={band.y}
        width={band.width}
        height={band.height}
        onmouseenter={(event) => onBandEnter(band, event)}
        onmouseleave={() => $custom?.hideTooltip()}
      />
    {/each}

    {#each bars as bar (bar.key)}
      <rect
        class="liquid-chart-bar"
        class:liquid-chart-dim={!bar.active}
        role="img"
        aria-label={`${bar.category}: ${bar.label} ${bar.value}`}
        x={bar.x}
        y={bar.y}
        width={bar.width}
        height={bar.height}
        fill={bar.color}
        rx="3"
        ry="3"
        pointer-events="none"
      />
    {/each}

    {#each valueLabels as lbl (lbl.key)}
      <text
        class="liquid-chart-value-label"
        x={lbl.x}
        y={lbl.y}
        text-anchor={lbl.anchor}
        dominant-baseline="middle"
        >{lbl.text}</text
      >
    {/each}
  </g>
{/if}

<style>
  .liquid-chart-grid {
    stroke: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    stroke-width: 1;
  }

  .liquid-chart-axis {
    fill: rgb(var(--color-surface-400));
    font-size: 0.62rem;
  }

  .liquid-chart-hit {
    fill: transparent;
    cursor: default;
  }

  .liquid-chart-bar {
    transition: opacity 120ms ease;
  }

  .liquid-chart-dim {
    opacity: 0.35;
  }

  .liquid-chart-value-label {
    fill: rgb(var(--color-surface-200));
    font-size: 0.58rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-chart-bar {
      transition: none;
    }
  }
</style>
