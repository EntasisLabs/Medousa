<script lang="ts">
  import LayoutRegionPane from "$lib/components/environment/LayoutRegionPane.svelte";
  import TilingNodeView from "$lib/components/environment/TilingNodeView.svelte";
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import { componentById } from "$lib/utils/layoutPresentation";
  import type { ComponentDef } from "$lib/types/environment";
  import type { TilingNode } from "$lib/utils/layoutTiling";

  interface Props {
    node: TilingNode;
    components: ComponentDef[];
    sessionId: string;
    profileId?: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
    editing?: boolean;
    depth?: number;
  }

  let {
    node,
    components,
    sessionId,
    profileId = null,
    feedStateForComponent,
    editing = false,
    depth = 0,
  }: Props = $props();

  const component = $derived(
    node.kind === "pane" && node.pane.componentId
      ? componentById(components, node.pane.componentId)
      : null,
  );
  const selected = $derived(editing && node.kind === "pane" && layoutEdit.selectedPaneId === node.pane.id);
  const compact = $derived(depth > 1);
</script>

{#if node.kind === "pane"}
  <LayoutRegionPane
    paneId={node.pane.id}
    {component}
    {selected}
    {editing}
    {sessionId}
    {profileId}
    {feedStateForComponent}
    {compact}
    onSelect={editing ? () => layoutEdit.selectPane(node.pane.id) : undefined}
  />
{:else}
  <div
    class="tiling-split"
    class:tiling-split-horizontal={node.direction === "horizontal"}
    class:tiling-split-vertical={node.direction === "vertical"}
  >
    <TilingNodeView
      node={node.first}
      {components}
      {sessionId}
      {profileId}
      {feedStateForComponent}
      {editing}
      depth={depth + 1}
    />
    <TilingNodeView
      node={node.second}
      {components}
      {sessionId}
      {profileId}
      {feedStateForComponent}
      {editing}
      depth={depth + 1}
    />
  </div>
{/if}

<style>
  .tiling-split {
    display: flex;
    flex: 1 1 0%;
    min-height: 0;
    min-width: 0;
    gap: 0.5rem;
  }

  .tiling-split-horizontal {
    flex-direction: row;
  }

  .tiling-split-vertical {
    flex-direction: column;
  }

  .tiling-split :global(> *) {
    flex: 1 1 0%;
    min-height: 0;
    min-width: 0;
  }
</style>
