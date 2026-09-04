<script lang="ts">
  /**
   * Inline sub-agent beat: a footnote-weight header that expands into the
   * worker's tool evidence and reasoning, without leaving the thread.
   */
  import { Bot, ChevronDown, Square } from "@lucide/svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import type { SubagentRow } from "$lib/utils/subagentRows";

  interface Props {
    row: SubagentRow;
    /** Open the full transcript overlay. */
    onOpen: () => void;
    onStop?: () => void;
    compact?: boolean;
  }

  let { row, onOpen, onStop, compact = false }: Props = $props();

  const badge = $derived(row.disposition === "bound" ? "Workshop" : "Peer");
  const executionTargetLabel = $derived(
    executionTargets.runtimeLabel(row.executionRuntimeId),
  );
  const thinking = $derived(row.thinking.trim());
  const hasEvidence = $derived(row.toolRuns.length > 0 || thinking.length > 0);
  const thoughtLabel = $derived(
    row.thinkingSeconds != null && row.thinkingSeconds >= 1
      ? `Thought for ${Math.round(row.thinkingSeconds)}s`
      : "Thinking",
  );
</script>

<details
  class="subagent-beat group/subagent"
  class:subagent-live={row.streaming}
  class:subagent-done={!row.streaming}
>
  <summary
    class="flex cursor-pointer list-none items-center gap-1.5 py-0.5 marker:content-none"
  >
    <span
      class="shrink-0 {row.streaming ? 'text-primary-400/80' : 'text-surface-700'}"
      aria-hidden="true"
    >
      <Bot size={12} strokeWidth={2} />
    </span>

    <span
      class="min-w-0 flex-1 truncate {compact ? 'text-[10px]' : 'text-[10px]'} {row.streaming
        ? 'text-content-tertiary'
        : 'text-content-faint'}"
    >
      <span class="text-content-quiet">{badge}</span>
      {#if executionTargetLabel}
        <span class="text-content-faint"> · </span>
        <span
          class="text-content-quiet"
          title={row.executionRuntimeId ?? undefined}
        >{row.streaming ? "Running on" : "Ran on"} {executionTargetLabel}</span>
      {/if}
      <span class="text-content-faint"> · </span>
      {row.title}
      {#if row.statusLine}
        <span class="text-content-faint"> · </span>
        <span
          class={row.streaming ? "text-content-quiet" : "text-content-faint"}
        >{row.statusLine}</span>
      {/if}
    </span>

    {#if row.streaming}
      <span
        class="h-1 w-1 shrink-0 animate-pulse rounded-full bg-primary-400"
        aria-hidden="true"
      ></span>
    {/if}

    {#if row.streaming && onStop}
      <button
        type="button"
        class="shrink-0 rounded p-0.5 text-content-faint transition-colors hover:text-content-secondary"
        title="Stop subagent"
        aria-label="Stop subagent"
        onclick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onStop?.();
        }}
      >
        <Square size={10} strokeWidth={2} />
      </button>
    {/if}

    <ChevronDown
      class="h-3 w-3 shrink-0 text-surface-700 transition-transform duration-200 group-open/subagent:rotate-180"
      strokeWidth={2}
      aria-hidden="true"
    />
  </summary>

  <div class="mt-1 space-y-1.5 border-l border-primary-500/15 pl-2.5">
    {#if row.toolRuns.length > 0}
      <ToolRunChips runs={row.toolRuns} compact inspectorCollapsed />
    {/if}

    {#if thinking}
      <details class="group/subagent-thinking">
        <summary
          class="flex cursor-pointer list-none items-center gap-1 py-0.5 text-[10px] text-content-faint marker:content-none hover:text-content-tertiary"
        >
          <span class="min-w-0 flex-1 truncate">{thoughtLabel}</span>
          <ChevronDown
            class="h-2.5 w-2.5 shrink-0 transition-transform duration-200 group-open/subagent-thinking:rotate-180"
            strokeWidth={2}
            aria-hidden="true"
          />
        </summary>
        <p class="whitespace-pre-wrap pt-1 text-[11px] leading-relaxed text-content-faint">
          {thinking}
        </p>
      </details>
    {/if}

    {#if !hasEvidence}
      <p class="text-[10px] text-content-faint">No tool activity yet.</p>
    {/if}

    <button
      type="button"
      class="workshop-text-action inline-flex items-center gap-1 text-[10px]"
      onclick={onOpen}
    >
      Open transcript
    </button>
  </div>
</details>

<style>
  /* Live: a touch of presence. Settled: footnote energy, same as Thinking. */
  .subagent-beat {
    margin-block: 0.35rem;
  }

  .subagent-done {
    opacity: 0.55;
  }

  .subagent-done:hover,
  .subagent-done[open] {
    opacity: 0.85;
  }
</style>
