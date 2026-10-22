<script lang="ts">
  import type { DrawCamera, DrawSize } from "$lib/draw/drawCamera";
  import type { LayoutGuide, LayoutPreferences } from "$lib/create/creativeLayout";
  let { camera, viewport, settings, guides, prefix }: { camera: DrawCamera; viewport: DrawSize; settings: LayoutPreferences; guides: LayoutGuide[]; prefix: string } = $props();
  const x = $derived(-camera.panX / camera.zoom), y = $derived(-camera.panY / camera.zoom);
  const step = $derived(settings.gridSize * 2 ** Math.max(0, Math.ceil(Math.log2(8 / (settings.gridSize * camera.zoom)))));
</script>

<g data-editor-only data-layout-overlay pointer-events="none" fill="none">
  {#if settings.grid !== "none"}
    <defs><pattern id={`${prefix}-grid`} x={settings.grid === "dots" ? -step / 2 : 0} y={settings.grid === "dots" ? -step / 2 : 0} width={step} height={step} patternUnits="userSpaceOnUse">
      {#if settings.grid === "dots"}<circle cx={step / 2} cy={step / 2} r={1 / camera.zoom} fill="#88929f" opacity=".5"/>
      {:else}<path d={`M ${step} 0 L 0 0 0 ${step}`} stroke="#88929f" stroke-width={.6 / camera.zoom} opacity=".35"/>{/if}
    </pattern></defs>
    <rect aria-label="Canvas grid" {x} {y} width={viewport.width / camera.zoom} height={viewport.height / camera.zoom} fill={`url(#${prefix}-grid)`}/>
  {/if}
  {#each guides as guide}
    <line aria-label="Alignment guide" x1={guide.axis === "x" ? guide.value : guide.from} y1={guide.axis === "y" ? guide.value : guide.from} x2={guide.axis === "x" ? guide.value : guide.to} y2={guide.axis === "y" ? guide.value : guide.to} stroke="var(--create-selection,#8e8bd6)" stroke-width={1 / camera.zoom}/>
  {/each}
</g>
