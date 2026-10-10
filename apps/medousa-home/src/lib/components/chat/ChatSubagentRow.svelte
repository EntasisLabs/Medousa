<script lang="ts">
  /** Workshop workers share the grouped agent row treatment. */
  import { ArrowUpRight, Bot, Square } from "@lucide/svelte";
  import AgentWorkContextLine from "./AgentWorkContextLine.svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import type { SubagentRow } from "$lib/utils/subagentRows";

  let { row, onOpen, onStop, compact = false }: {
    row: SubagentRow;
    onOpen: () => void;
    onStop?: () => void;
    compact?: boolean;
  } = $props();

  const badge = $derived(row.disposition === "bound" ? "Workshop agent" : "Peer agent");
  const executionTargetLabel = $derived(executionTargets.runtimeLabel(row.executionRuntimeId));
  const thoughtLabel = $derived(row.thinkingSeconds != null && row.thinkingSeconds >= 1
    ? `Thought for ${Math.round(row.thinkingSeconds)}s` : row.streaming ? "Thinking now" : null);
  const statusLabel = $derived(row.attention ? row.statusLine || "Needs attention" : row.streaming ? "Agent working" : row.statusLine || "Complete");
</script>

<article class="peer-context" class:compact data-worker-id={row.workId}>
  <AgentWorkContextLine title={row.title} status={statusLabel} activity={row.streaming ? row.statusLine : null} attention={row.attention} state={row.attention ? "attention" : row.terminal ? "complete" : "working"}>
    <div class="peer-info">
      <Bot size={16} aria-hidden="true" />
      <div><p>{badge}</p><small>{[executionTargetLabel, row.model].filter(Boolean).join(' · ')}</small></div>
      <button type="button" onclick={onOpen}>View transcript <ArrowUpRight size={13} aria-hidden="true" /></button>
    </div>
    {#if row.toolRuns.length > 0}<ToolRunChips runs={row.toolRuns} compact inspectorCollapsed />{/if}
    <div class="peer-footer">
      {#if thoughtLabel}<span>{thoughtLabel}</span>{/if}
      {#if row.streaming && onStop}
        <button type="button" onclick={() => onStop?.()}><Square size={11} aria-hidden="true" /> Stop agent</button>
      {/if}
    </div>
  </AgentWorkContextLine>
</article>

<style>
  .peer-context { min-width: 0; border-top: 1px solid rgb(var(--theme-border) / .6); }
  .peer-info { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .peer-info > :global(svg) { color: rgb(var(--theme-text-tertiary)); flex-shrink: 0; }
  .peer-info div { min-width: 0; flex: 1; }
  .peer-info p { font-size: 13px; color: rgb(var(--theme-text-primary)); }
  .peer-info small { font-size: 12px; color: rgb(var(--theme-text-secondary)); overflow-wrap: anywhere; }
  button { display: inline-flex; align-items: center; gap: 5px; border: 0; background: transparent; color: rgb(var(--theme-text-secondary)); padding: 5px 0; font-size: 12px; }
  button:hover { color: rgb(var(--theme-text-primary)); }
  .peer-footer { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; margin-top: 8px; color: rgb(var(--theme-text-tertiary)); font-size: 12px; }
  @media (max-width: 480px) { .peer-info { flex-wrap: wrap; } .peer-info div { flex-basis: calc(100% - 26px); } .peer-info button { margin-left: 26px; } }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
