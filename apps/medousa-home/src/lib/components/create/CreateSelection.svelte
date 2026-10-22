<script lang="ts">
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  import { selectionRoots, worldMatrix, nodeDimensions } from "$lib/create/creativeScene";
  import { textMetricsVersion } from "$lib/create/creativeText";
  import { selectionBounds } from "$lib/create/creativeSelection";
  let { nodes, selected, zoom, disabled = false }: { nodes: CreativeNode[]; selected: string[]; zoom: number; disabled?: boolean } = $props();
  const roots = $derived(selectionRoots(selected, nodes));
  const single = $derived(roots.length === 1 && roots[0].content.kind !== "group" ? roots[0] : null);
  const bounds = $derived.by(() => { void $textMetricsVersion; return selectionBounds(selected, nodes); });
  const dimensions = $derived.by(() => { void $textMetricsVersion; return single ? nodeDimensions(single) : null; });
  const width = $derived(dimensions ? dimensions.width : bounds?.width ?? 0);
  const height = $derived(dimensions ? dimensions.height : bounds?.height ?? 0);
  const matrix = $derived(single ? worldMatrix(single, nodes) : [1, 0, 0, 1, bounds?.x ?? 0, bounds?.y ?? 0]);
  const handles = $derived([
    { id: "se", x: width, y: height, cursor: "nwse-resize" },
    { id: "nw", x: 0, y: 0, cursor: "nwse-resize" },
    { id: "ne", x: width, y: 0, cursor: "nesw-resize" },
    { id: "sw", x: 0, y: height, cursor: "nesw-resize" },
    ...(single && single.content.kind !== "text" ? [
      { id: "n", x: width / 2, y: 0, cursor: "ns-resize" },
      { id: "e", x: width, y: height / 2, cursor: "ew-resize" },
      { id: "s", x: width / 2, y: height, cursor: "ns-resize" },
      { id: "w", x: 0, y: height / 2, cursor: "ew-resize" },
    ] : []),
  ]);
</script>

{#if bounds && roots.length}
  <g data-editor-only data-selection-box aria-label="Selection bounds" transform={`matrix(${matrix.join(" ")})`} pointer-events="none" fill="none" stroke="var(--create-selection,#8e8bd6)" stroke-width={1.25 / zoom}>
    <rect {width} {height} />
    {#if !disabled}
      {#each handles as handle}
        <rect data-resize="selection" data-handle={handle.id} x={handle.x - 4 / zoom} y={handle.y - 4 / zoom} width={8 / zoom} height={8 / zoom} rx={1 / zoom} fill="var(--create-handle,#fff)" pointer-events="all" style:cursor={handle.cursor} />
      {/each}
    {/if}
  </g>
{/if}
