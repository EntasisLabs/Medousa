<script lang="ts">
  import { onMount } from "svelte";
  import { cornerRadius, hasCorners } from "$lib/create/creativeAppearance";
  import { resizeCursor, rotationCursor } from "$lib/create/creativeCursors";
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  import { selectionRoots } from "$lib/create/creativeScene";
  import { textMetricsVersion } from "$lib/create/creativeText";
  import { lineGeometry } from "$lib/create/creativeLine";
  import { selectionFrame, type SelectionFrame } from "$lib/create/creativeSelection";
  let { nodes, selected, zoom, disabled = false, frameOverride }: { nodes: CreativeNode[]; selected: string[]; zoom: number; disabled?: boolean; frameOverride?: SelectionFrame } = $props();
  let coarse = $state(false);
  onMount(() => { const query=matchMedia("(any-pointer: coarse)");const update=()=>coarse=query.matches;update();query.addEventListener("change",update);return()=>query.removeEventListener("change",update); });
  const hitSize = $derived((coarse ? 44 : 24) / zoom);
  const roots = $derived(selectionRoots(selected, nodes));
  const frame = $derived.by(() => { void $textMetricsVersion; return frameOverride ?? selectionFrame(selected, nodes); });
  const single = $derived(roots.length === 1 ? roots[0] : null);
  const connector = $derived(single ? lineGeometry(single) : null);
  const width = $derived(frame?.width ?? 0), height = $derived(frame?.height ?? 0);
  const handles = $derived([
    { id: "se", x: width, y: height, cursor: "nwse-resize" },
    { id: "nw", x: 0, y: 0, cursor: "nwse-resize" },
    { id: "ne", x: width, y: 0, cursor: "nesw-resize" },
    { id: "sw", x: 0, y: height, cursor: "nesw-resize" },
    ...(single && single.content.kind !== "group" ? [
      ...(single.content.kind === "text" ? [] : [{ id: "n", x: width / 2, y: 0, cursor: "ns-resize" }, { id: "s", x: width / 2, y: height, cursor: "ns-resize" }]),
      { id: "e", x: width, y: height / 2, cursor: "ew-resize" },
      { id: "w", x: 0, y: height / 2, cursor: "ew-resize" },
    ] : []),
  ]);
</script>

{#if frame && roots.length}
  <g data-editor-only data-selection-box aria-label="Selection bounds" transform={`matrix(${frame.matrix.join(" ")})`} pointer-events="none" fill="none" stroke="var(--create-selection,#8e8bd6)" stroke-width={1.25 / zoom}>
    {#if connector}
      <line x1="0" y1="0" x2={width} y2="0" opacity=".6"/>
      {#if !disabled}
        {#each ["start", "end"] as endpoint}
          <circle data-endpoint={endpoint} cx={endpoint === "start" ? 0 : width} cy="0" r={hitSize / 2} fill="transparent" stroke="none" pointer-events="all" style:cursor="crosshair"/>
          <circle aria-label={`${endpoint} endpoint`} cx={endpoint === "start" ? 0 : width} cy="0" r={6 / zoom} fill="var(--create-handle,#fff)" pointer-events="none" style:cursor="crosshair"/>
        {/each}
      {/if}
    {:else}
      <rect {width} {height}/>
      {#if !disabled}{#each handles as handle}
        <rect data-resize="selection" data-handle={handle.id} x={handle.x-hitSize/2} y={handle.y-hitSize/2} width={hitSize} height={hitSize} fill="transparent" stroke="none" pointer-events="all" style:cursor={resizeCursor(handle.id,frame.angle)}/>
        <rect x={handle.x - 4 / zoom} y={handle.y - 4 / zoom} width={8 / zoom} height={8 / zoom} rx={1 / zoom} fill="var(--create-handle,#fff)" pointer-events="none" style:cursor={resizeCursor(handle.id, frame.angle)}/>
      {/each}{/if}
    {/if}
    {#if !disabled && single && hasCorners(single)}
      {@const inset = Math.max(12 / zoom, cornerRadius(single))}
      <circle data-radius="selection" cx={Math.min(width/2,inset)} cy={Math.min(height/2,inset)} r={hitSize/2} fill="transparent" stroke="none" pointer-events="all" style:cursor="ew-resize"/>
      <circle aria-label="Corner radius" cx={Math.min(width / 2, inset)} cy={Math.min(height / 2, inset)} r={4 / zoom} fill="var(--create-handle,#fff)" pointer-events="none" style:cursor="ew-resize"/>
    {/if}
    {#if !disabled}
      <line x1={width / 2} y1="0" x2={width / 2} y2={-28 / zoom}/>
      <circle data-rotate="selection" cx={width/2} cy={-28/zoom} r={hitSize/2} fill="transparent" stroke="none" pointer-events="all" style:cursor={rotationCursor}/>
      <circle aria-label="Rotate selection" cx={width / 2} cy={-28 / zoom} r={5 / zoom} fill="var(--create-handle,#fff)" pointer-events="none" style:cursor={rotationCursor}/>
    {/if}
  </g>
{/if}
