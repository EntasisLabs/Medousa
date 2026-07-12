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
  /** Match composer icon optical size (~16px). */
  const ringRadius = 7.25;
  const ringCircumference = 2 * Math.PI * ringRadius;
  const ringOffset = $derived(
    fillPct != null ? ringCircumference * (1 - fillPct / 100) : ringCircumference,
  );
  const pressure = $derived(
    fillPct == null ? "calm" : fillPct >= 90 ? "hot" : fillPct >= 75 ? "warm" : "calm",
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
  <div
    class="context-usage-root"
    class:context-usage-root-compact={compact}
    class:context-usage-root-open={open}
    data-pressure={pressure}
    bind:this={rootEl}
  >
    {#if open}
      <div class="context-usage-panel" role="dialog" aria-label="Context usage">
        <header class="context-usage-panel-header">
          <div class="min-w-0">
            <p class="context-usage-panel-title">Context</p>
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
      aria-label={fillPct != null ? `Context usage ${fillPct}%` : "Context usage"}
      title={summary || "Context usage"}
      aria-expanded={open}
      onclick={toggle}
    >
      <svg class="context-usage-ring" viewBox="0 0 20 20" aria-hidden="true">
        <circle class="context-usage-ring-track" cx="10" cy="10" r={ringRadius} />
        <circle
          class="context-usage-ring-fill"
          cx="10"
          cy="10"
          r={ringRadius}
          stroke-dasharray={ringCircumference}
          stroke-dashoffset={ringOffset}
        />
      </svg>
      <span class="context-usage-trigger-label">
        {fillPct != null ? `${fillPct}%` : "—"}
      </span>
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
    width: min(300px, calc(100vw - 24px));
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.35);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.98);
    box-shadow: 0 14px 36px rgb(0 0 0 / 0.35);
    padding: 0.75rem 0.85rem;
    backdrop-filter: blur(12px);
  }

  .context-usage-panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.65rem;
  }

  .context-usage-panel-title {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .context-usage-panel-summary {
    margin: 0.15rem 0 0;
    font-size: 0.6875rem;
    line-height: 1.35;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .context-usage-panel-close {
    border: none;
    background: transparent;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    font-size: 1.05rem;
    line-height: 1;
    padding: 0 0.1rem;
    cursor: pointer;
    opacity: 0.7;
    transition: opacity 120ms ease, color 120ms ease;
  }

  .context-usage-panel-close:hover {
    opacity: 1;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .context-usage-bar {
    display: flex;
    height: 5px;
    overflow: hidden;
    border-radius: 999px;
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.85);
    margin-bottom: 0.65rem;
  }

  .context-usage-bar-segment {
    display: block;
    height: 100%;
    min-width: 1px;
  }

  .context-usage-layers {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    max-height: 13rem;
    overflow: auto;
  }

  .context-usage-layer-row {
    display: grid;
    grid-template-columns: 8px 1fr auto;
    gap: 0.5rem;
    align-items: center;
    font-size: 0.71875rem;
  }

  .context-usage-layer-swatch {
    width: 7px;
    height: 7px;
    border-radius: 2px;
  }

  .context-usage-layer-label {
    color: rgb(var(--shell-whisper, var(--color-surface-200)));
  }

  .context-usage-layer-value {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    font-variant-numeric: tabular-nums;
  }

  .context-usage-footnote {
    margin: 0.65rem 0 0;
    font-size: 0.625rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.9);
  }

  /* Industry pattern: thin ring + percent beside it — sized to composer icons */
  .context-usage-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    height: 2rem; /* match .composer-bar-icon-btn h-8 */
    padding: 0 0.35rem;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
    cursor: pointer;
    transition:
      color 140ms ease,
      background 140ms ease;
  }

  .context-usage-trigger:hover,
  .context-usage-root-open .context-usage-trigger {
    color: rgb(var(--shell-icon-hover, var(--color-surface-200)));
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.5);
  }

  .context-usage-root-compact .context-usage-trigger {
    height: 2rem;
    gap: 0.35rem;
    padding: 0 0.3rem;
  }

  .context-usage-ring {
    display: block;
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    transform: rotate(-90deg);
  }

  .context-usage-root-compact .context-usage-ring {
    width: 0.9375rem;
    height: 0.9375rem;
  }

  .context-usage-ring-track {
    fill: none;
    stroke: rgb(var(--shell-border, var(--color-surface-500)) / 0.6);
    stroke-width: 2;
  }

  .context-usage-ring-fill {
    fill: none;
    stroke: rgb(var(--shell-icon-hover, var(--color-surface-200)));
    stroke-width: 2;
    stroke-linecap: round;
    transition: stroke-dashoffset 220ms ease, stroke 160ms ease;
  }

  .context-usage-root[data-pressure="warm"] .context-usage-ring-fill {
    stroke: rgb(var(--color-warning-400));
  }

  .context-usage-root[data-pressure="hot"] .context-usage-ring-fill {
    stroke: rgb(var(--color-error-400));
  }

  .context-usage-trigger-label {
    display: block;
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: -0.015em;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    /* Optical center with the ring (caps sit slightly low in most UIs). */
    transform: translateY(-0.5px);
  }

  .context-usage-root-compact .context-usage-trigger-label {
    font-size: 0.71875rem;
  }

  .context-usage-root[data-pressure="warm"] .context-usage-trigger-label {
    color: rgb(var(--color-warning-300));
  }

  .context-usage-root[data-pressure="hot"] .context-usage-trigger-label {
    color: rgb(var(--color-error-300));
  }
</style>
