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
</script>

{#if runs.length > 0}
  <ToolRunChips
    {runs}
    sessionId={ctx.sessionId}
    {turnIndex}
    onPromoteToFlow={ctx.onPromoteToFlow}
    compact={ctx.mobile ?? false}
    inspectorCollapsed={!streaming}
  />
{/if}
