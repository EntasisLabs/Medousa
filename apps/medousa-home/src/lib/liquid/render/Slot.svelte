<script lang="ts">
  /**
   * Renders an ordered list of child nodes. Keyed by `node.id` — this is the
   * reconciliation key that preserves each child's Svelte instance and local
   * state (cursor, scroll, selection) across `fill_slot` / `patch_props`.
   */
  import type { SceneNode } from "$lib/liquid/core";
  import SceneRenderer from "./SceneRenderer.svelte";

  interface Props {
    nodes: SceneNode[];
  }
  let { nodes }: Props = $props();
</script>

{#each nodes as child (child.id)}
  <SceneRenderer node={child} />
{/each}
