<script lang="ts">
  /** `tool_trace` shell archetype — reuses the tool-run lineage chips. */
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import type { ToolRunState } from "$lib/types/chat";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const runs = $derived(Array.isArray(node.props.runs) ? (node.props.runs as ToolRunState[]) : []);
  const turnIndex = $derived(typeof node.props.turnIndex === "number" ? node.props.turnIndex : null);
  const streaming = $derived(node.props.streaming === true);
  const compact = $derived(node.props.compact === true || (ctx.mobile ?? false));
</script>

{#if runs.length > 0}
  <div class="liquid-tool-trace" class:liquid-tool-trace-compact={compact && !streaming}>
    <ToolRunChips
      {runs}
      sessionId={ctx.sessionId}
      {turnIndex}
      onPromoteToFlow={ctx.onPromoteToFlow}
      {compact}
      inspectorCollapsed={!streaming}
    />
  </div>
{/if}

<style>
  .liquid-tool-trace {
    margin-top: 0.75rem;
  }

  /* Settled host-lane: footnote energy — less vertical weight. */
  .liquid-tool-trace-compact {
    margin-top: 0.5rem;
    opacity: 0.9;
  }
</style>
