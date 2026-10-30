<script lang="ts">
  import type { SceneNode } from '$lib/liquid/core';
  let { node }: { node: SceneNode; context?: unknown } = $props();
</script>
{#if node.slots?.flow}
  {#each node.slots.flow as child (child.id)}<p data-scene-id={child.id}>{child.id}</p>{/each}
{:else}
  <p data-scene-id={node.id}>{node.id}</p>
{/if}
