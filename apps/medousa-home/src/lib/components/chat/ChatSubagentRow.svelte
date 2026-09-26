<script lang="ts">
  /** A first-class peer handoff card anchored inside the parent conversation. */
  import {
    Bot,
    Check,
    ChevronRight,
    LoaderCircle,
    MessageSquareText,
    Square,
  } from "@lucide/svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import type { SubagentRow } from "$lib/utils/subagentRows";

  interface Props {
    row: SubagentRow;
    onOpen: () => void;
    onStop?: () => void;
    compact?: boolean;
  }

  let { row, onOpen, onStop, compact = false }: Props = $props();

  const badge = $derived(row.disposition === "bound" ? "Workshop agent" : "Peer agent");
  const executionTargetLabel = $derived(
    executionTargets.runtimeLabel(row.executionRuntimeId),
  );
  const thoughtLabel = $derived(
    row.thinkingSeconds != null && row.thinkingSeconds >= 1
      ? `Thought for ${Math.round(row.thinkingSeconds)}s`
      : row.streaming
        ? "Thinking now"
        : null,
  );
  const statusLabel = $derived(row.streaming ? "Working" : row.statusLine || "Complete");
</script>

<article
  class="peer-card {row.streaming ? 'peer-card-live' : 'peer-card-done'} {compact ? 'peer-card-compact' : ''}"
>
  <header class="peer-card-header">
    <span class="peer-card-avatar" aria-hidden="true">
      <Bot size={15} strokeWidth={1.9} />
    </span>

    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-2">
        <span class="peer-card-kind">{badge}</span>
        <span class="peer-card-status" class:peer-card-status-live={row.streaming}>
          {#if row.streaming}
            <LoaderCircle class="h-2.5 w-2.5 animate-spin" strokeWidth={2.2} />
          {:else}
            <Check class="h-2.5 w-2.5" strokeWidth={2.4} />
          {/if}
          {statusLabel}
        </span>
      </div>
      <p class="peer-card-title">{row.title}</p>
    </div>

    {#if row.streaming && onStop}
      <button
        type="button"
        class="peer-card-stop"
        title="Stop peer"
        aria-label="Stop peer"
        onclick={() => onStop?.()}
      >
        <Square size={11} strokeWidth={2} />
      </button>
    {/if}
  </header>

  {#if executionTargetLabel || row.model}
    <p class="peer-card-meta">
      {#if executionTargetLabel}
        <span title={row.executionRuntimeId ?? undefined}>{executionTargetLabel}</span>
      {/if}
      {#if executionTargetLabel && row.model}<span aria-hidden="true">·</span>{/if}
      {#if row.model}<span>{row.model}</span>{/if}
    </p>
  {/if}

  {#if row.toolRuns.length > 0}
    <div class="peer-card-tools">
      <ToolRunChips runs={row.toolRuns} compact inspectorCollapsed />
    </div>
  {/if}

  <footer class="peer-card-footer">
    <span class="peer-card-duration">
      {thoughtLabel ?? (row.toolRuns.length > 0 ? `${row.toolRuns.length} tool run${row.toolRuns.length === 1 ? "" : "s"}` : "No tool activity")}
    </span>
    <button type="button" class="peer-card-open" onclick={onOpen}>
      <MessageSquareText size={13} strokeWidth={2} aria-hidden="true" />
      View transcript
      <ChevronRight size={12} strokeWidth={2.2} aria-hidden="true" />
    </button>
  </footer>
</article>

<style>
  .peer-card {
    position: relative;
    margin-block: 0.75rem;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, rgb(var(--color-primary-400)) 22%, transparent);
    border-radius: 0.9rem;
    padding: 0.8rem 0.85rem 0.7rem;
    background:
      linear-gradient(135deg, color-mix(in srgb, rgb(var(--color-primary-500)) 9%, transparent), transparent 58%),
      color-mix(in srgb, rgb(var(--color-surface-900)) 72%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, white 4%, transparent);
  }

  .peer-card-live {
    border-color: color-mix(in srgb, rgb(var(--color-primary-400)) 38%, transparent);
    box-shadow:
      inset 0 1px 0 color-mix(in srgb, white 5%, transparent),
      0 10px 30px color-mix(in srgb, rgb(var(--color-surface-950)) 20%, transparent);
  }

  .peer-card-header {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
  }

  .peer-card-avatar {
    display: inline-flex;
    width: 1.85rem;
    height: 1.85rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 1px solid color-mix(in srgb, rgb(var(--color-primary-400)) 24%, transparent);
    border-radius: 0.55rem;
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 12%, transparent);
    color: rgb(var(--color-primary-300));
  }

  .peer-card-kind {
    overflow: hidden;
    color: rgb(var(--color-surface-300));
    font-size: 0.68rem;
    font-weight: 650;
    letter-spacing: 0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .peer-card-status {
    display: inline-flex;
    max-width: 10rem;
    align-items: center;
    gap: 0.2rem;
    overflow: hidden;
    border-radius: 999px;
    padding: 0.12rem 0.4rem;
    background: color-mix(in srgb, rgb(var(--color-success-500)) 11%, transparent);
    color: color-mix(in srgb, rgb(var(--color-success-300)) 82%, white);
    font-size: 0.59rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .peer-card-status-live {
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 14%, transparent);
    color: rgb(var(--color-primary-300));
  }

  .peer-card-title {
    display: -webkit-box;
    margin: 0.22rem 0 0;
    overflow: hidden;
    color: rgb(var(--color-surface-100));
    font-size: 0.78rem;
    font-weight: 560;
    line-height: 1.38;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .peer-card-stop {
    display: inline-flex;
    width: 1.65rem;
    height: 1.65rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
  }

  .peer-card-stop:hover {
    background: color-mix(in srgb, rgb(var(--color-error-500)) 12%, transparent);
    color: rgb(var(--color-error-300));
  }

  .peer-card-meta {
    display: flex;
    gap: 0.35rem;
    margin: 0.45rem 0 0 2.5rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.62rem;
  }

  .peer-card-tools {
    margin-top: 0.65rem;
  }

  .peer-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-top: 0.65rem;
    padding-top: 0.55rem;
    border-top: 1px solid color-mix(in srgb, rgb(var(--color-surface-500)) 12%, transparent);
  }

  .peer-card-duration {
    min-width: 0;
    overflow: hidden;
    color: rgb(var(--color-surface-500));
    font-size: 0.62rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .peer-card-open {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.32rem;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.32rem 0.45rem;
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 11%, transparent);
    color: rgb(var(--color-primary-300));
    font-size: 0.66rem;
    font-weight: 600;
    cursor: pointer;
  }

  .peer-card-open:hover {
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 18%, transparent);
    color: rgb(var(--color-primary-200));
  }

  .peer-card-compact {
    margin-block: 0.5rem;
    padding: 0.65rem 0.7rem 0.6rem;
  }

  @media (max-width: 520px) {
    .peer-card {
      border-radius: 0.8rem;
    }

    .peer-card-meta {
      margin-left: 0;
    }
  }
</style>
