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
  const compact = $derived(node.props.compact === true || (ctx.mobile ?? false));
</script>

{#if runs.length > 0}
  <div class="liquid-tool-trace" class:liquid-tool-trace-compact={compact}>
    <ToolRunChips
      {runs}
      sessionId={ctx.sessionId}
      {turnIndex}
      onPromoteToFlow={ctx.onPromoteToFlow}
      {compact}
      inspectorCollapsed
    />
  </div>
{/if}

<style>
  .liquid-tool-trace {
    margin-top: 0.75rem;
  }

  /* Keep compact activity spacing stable while running and settled. */
  .liquid-tool-trace-compact {
    margin-top: 0.5rem;
  }
</style>
