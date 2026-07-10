<script lang="ts">
  /** `thinking` shell archetype — reuses the existing reasoning trace. */
  import AssistantThinking from "$lib/components/chat/AssistantThinking.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const reasoning = $derived(typeof node.props.reasoning === "string" ? node.props.reasoning : "");
  const streaming = $derived(node.props.streaming === true);
</script>

{#if reasoning.trim()}
  <AssistantThinking {reasoning} {streaming} compact={ctx.mobile ?? false} />
{/if}
