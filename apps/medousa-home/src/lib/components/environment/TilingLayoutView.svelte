<script lang="ts">
  import TilingNodeView from "$lib/components/environment/TilingNodeView.svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import type { TilingNode } from "$lib/utils/layoutTiling";

  interface Props {
    root: TilingNode;
    components: ComponentDef[];
    sessionId: string;
    profileId?: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
    editing?: boolean;
    padded?: boolean;
  }

  let {
    root,
    components,
    sessionId,
    profileId = null,
    feedStateForComponent,
    editing = false,
    padded = false,
  }: Props = $props();
</script>

<div
  class="tiling-layout-view"
  class:tiling-layout-view-editing={editing}
  class:tiling-layout-view-padded={padded}
>
  <TilingNodeView
    node={root}
    {components}
    {sessionId}
    {profileId}
    {feedStateForComponent}
    {editing}
  />
</div>

<style>
  .tiling-layout-view {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .tiling-layout-view-padded {
    padding: 0.5rem;
  }

  .tiling-layout-view :global(> *) {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
  }
</style>
