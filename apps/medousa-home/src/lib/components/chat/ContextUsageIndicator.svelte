<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import {
    contextUsageSegments,
    formatTokenCount,
    usageFillPercent,
    usageSummaryLine,
  } from "$lib/utils/contextUsage";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  let open = $state(false);
  let rootEl: HTMLDivElement | undefined = $state();

  const report = $derived(chat.contextUsage);
  const visible = $derived(report != null);
  const fillPct = $derived(report ? usageFillPercent(report) : null);
  const segments = $derived(report ? contextUsageSegments(report) : []);
  const summary = $derived(report ? usageSummaryLine(report) : "");
  const ringCircumference = 2 * Math.PI * 15.5;
  const ringOffset = $derived(
    fillPct != null ? ringCircumference * (1 - fillPct / 100) : ringCircumference,
  );

  function toggle() {
    if (!report) return;
    open = !open;
    chat.contextUsagePanelOpen = open;
  }

  function close() {
    open = false;
    chat.contextUsagePanelOpen = false;
  }

  $effect(() => {
    if (chat.contextUsagePanelOpen && report) {
      open = true;
    }
    if (!chat.contextUsagePanelOpen) {
      open = false;
    }
  });

  $effect(() => {
    if (!open) return;
    const onDocClick = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (rootEl && target && !rootEl.contains(target)) {
        close();
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("click", onDocClick, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick, true);
      document.removeEventListener("keydown", onKey);
    };
  });
</script>

{#if visible && report}
  <div class="context-usage-root" class:context-usage-root-compact={compact} bind:this={rootEl}>
    {#if open}
      <div class="context-usage-panel" role="dialog" aria-label="Context usage">
        <header class="context-usage-panel-header">
          <div>
            <p class="context-usage-panel-title">Context usage</p>
            <p class="context-usage-panel-summary">{summary}</p>
          </div>
          <button type="button" class="context-usage-panel-close" aria-label="Close" onclick={close}>
            ×
          </button>
        </header>

        <div
          class="context-usage-bar"
          role="img"
          aria-label={`Context window ${fillPct ?? 0}% full`}
        >
          {#each segments as segment (segment.layer.id)}
            <span
              class="context-usage-bar-segment"
              style:width="{segment.widthPct}%"
              style:background-color={segment.color}
              title="{segment.layer.label}: {formatTokenCount(segment.layer.tokens_estimate)}"
            ></span>
          {/each}
        </div>

        <ul class="context-usage-layers">
          {#each report.layers as layer (layer.id)}
            <li class="context-usage-layer-row">
              <span
                class="context-usage-layer-swatch"
                style:background-color={segments.find((s) => s.layer.id === layer.id)?.color}
              ></span>
              <span class="context-usage-layer-label">{layer.label}</span>
              <span class="context-usage-layer-value">{formatTokenCount(layer.tokens_estimate)}</span>
            </li>
          {/each}
        </ul>

        <p class="context-usage-footnote">
          Estimator: {report.estimator}
          {#if report.tool_count > 0}
            · {report.tool_count} tools
          {/if}
        </p>
      </div>
    {/if}

    <button
      type="button"
      class="context-usage-trigger"
      aria-label="Context usage"
      aria-expanded={open}
      onclick={toggle}
    >
      <svg class="context-usage-ring" viewBox="0 0 36 36" aria-hidden="true">
        <circle class="context-usage-ring-track" cx="18" cy="18" r="15.5" />
        <circle
          class="context-usage-ring-fill"
          cx="18"
          cy="18"
          r="15.5"
          stroke-dasharray={ringCircumference}
          stroke-dashoffset={ringOffset}
        />
      </svg>
      <span class="context-usage-trigger-label">{fillPct != null ? `${fillPct}%` : "ctx"}</span>
    </button>
  </div>
{/if}

<style>
  .context-usage-root {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .context-usage-panel {
    position: absolute;
    right: 0;
    bottom: calc(100% + 10px);
    z-index: 40;
    width: min(320px, calc(100vw - 24px));
    border-radius: 12px;
    border: 1px solid rgb(100 116 139 / 0.35);
    background: rgb(15 23 42 / 0.98);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.45);
    padding: 12px 14px;
  }

  .context-usage-panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 10px;
  }

  .context-usage-panel-title {
    font-size: 13px;
    font-weight: 600;
    color: rgb(248 250 252);
  }

  .context-usage-panel-summary {
    margin-top: 2px;
    font-size: 11px;
    color: rgb(148 163 184);
  }

  .context-usage-panel-close {
    border: none;
    background: transparent;
    color: rgb(148 163 184);
    font-size: 18px;
    line-height: 1;
    padding: 0 2px;
    cursor: pointer;
  }

  .context-usage-bar {
    display: flex;
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: rgb(51 65 85 / 0.8);
    margin-bottom: 10px;
  }

  .context-usage-bar-segment {
    display: block;
    height: 100%;
    min-width: 1px;
  }

  .context-usage-layers {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 220px;
    overflow: auto;
  }

  .context-usage-layer-row {
    display: grid;
    grid-template-columns: 10px 1fr auto;
    gap: 8px;
    align-items: center;
    font-size: 12px;
  }

  .context-usage-layer-swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }

  .context-usage-layer-label {
    color: rgb(226 232 240);
  }

  .context-usage-layer-value {
    color: rgb(148 163 184);
    font-variant-numeric: tabular-nums;
  }

  .context-usage-footnote {
    margin-top: 10px;
    font-size: 10px;
    color: rgb(100 116 139);
  }

  .context-usage-trigger {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: none;
    border-radius: 999px;
    background: rgb(30 41 59 / 0.9);
    color: rgb(203 213 225);
    cursor: pointer;
  }

  .context-usage-root-compact .context-usage-trigger {
    width: 30px;
    height: 30px;
  }

  .context-usage-ring {
    position: absolute;
    inset: 2px;
    transform: rotate(-90deg);
  }

  .context-usage-ring-track {
    fill: none;
    stroke: rgb(51 65 85);
    stroke-width: 3;
  }

  .context-usage-ring-fill {
    fill: none;
    stroke: rgb(96 165 250);
    stroke-width: 3;
    stroke-linecap: round;
  }

  .context-usage-trigger-label {
    position: relative;
    z-index: 1;
    font-size: 9px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
