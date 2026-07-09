<script lang="ts">
  /** `presentation` shell archetype — reuses the artifact strip (iframe isolation). */
  import ChatArtifactStrip from "$lib/components/chat/ChatArtifactStrip.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import type { UiArtifact } from "$lib/types/chat";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const artifacts = $derived(
    Array.isArray(node.props.artifacts) ? (node.props.artifacts as UiArtifact[]) : [],
  );
</script>

{#if artifacts.length > 0 && ctx.sessionId}
  <ChatArtifactStrip
    sessionId={ctx.sessionId}
    {artifacts}
    compact={(ctx.mobile ?? false) || (ctx.compact ?? false)}
  />
{/if}
