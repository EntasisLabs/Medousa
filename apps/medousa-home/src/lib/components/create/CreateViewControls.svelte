<script lang="ts">
  import { ChevronDown, Grid2X2 } from "@lucide/svelte";
  import type { LayoutPreferences } from "$lib/create/creativeLayout";
  let { settings = $bindable(), onfit, onselection, hasSelection }: { settings: LayoutPreferences; onfit: () => void; onselection: () => void; hasSelection: boolean } = $props();
  let menu = $state<HTMLDetailsElement>();
  function outside(event: PointerEvent) { if (menu?.open && event.target instanceof Node && !menu.contains(event.target)) menu.open = false; }
  function escape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    const details = (event.target as Element).closest("details");
    if (details) { event.stopPropagation(); details.open = false; details.querySelector("summary")?.focus(); }
  }
</script>

<svelte:window onpointerdowncapture={outside}/>

<details bind:this={menu} class="layout-view">
  <summary onkeydown={escape} aria-label="Canvas view options"><Grid2X2 size={13}/>View<ChevronDown size={11}/></summary>
  <div class="layout-view-menu" role="dialog" aria-label="Canvas view settings" tabindex="-1" onkeydown={escape}>
    <label>Grid<select aria-label="Grid appearance" bind:value={settings.grid}><option value="none">Off</option><option value="dots">Dots</option><option value="lines">Lines</option></select></label>
    <label>Grid spacing<select aria-label="Grid spacing" bind:value={settings.gridSize}>{#each [8, 16, 32, 64] as value}<option {value}>{value} px</option>{/each}</select></label>
    <label><input type="checkbox" bind:checked={settings.snapGrid}/>Snap to grid</label>
    <label><input type="checkbox" bind:checked={settings.smartGuides}/>Smart guides &amp; snapping</label>
    <p>Hold Ctrl while dragging to ignore snapping.</p>
    <hr/>
    <button type="button" onclick={(e) => { onfit(); e.currentTarget.closest("details")?.removeAttribute("open"); }}>Fit canvas</button>
    <button type="button" disabled={!hasSelection} onclick={(e) => { onselection(); e.currentTarget.closest("details")?.removeAttribute("open"); }}>Zoom to selection</button>
  </div>
</details>

<style>
  .layout-view { position: relative; pointer-events: auto; }
  summary { display: flex; align-items: center; gap: 4px; padding: 6px 8px; border: 1px solid var(--create-line); border-radius: 7px; background: var(--create-raised); cursor: pointer; list-style: none; }
  summary::-webkit-details-marker { display: none; }
  .layout-view-menu { position: absolute; right: 0; top: calc(100% + 6px); width: min(235px, calc(100vw - 36px)); padding: 10px; border: 1px solid var(--create-line); background: var(--create-raised); border-radius: 9px; box-shadow: 0 6px 20px #0002; color: var(--create-text); }
  label { display: flex; align-items: center; gap: 8px; padding: 7px 2px; }
  select { margin-left: auto; background: var(--create-soft); color: inherit; border: 0; border-radius: 5px; padding: 5px; }
  input { accent-color: var(--create-selection); }
  p { font-size: 10px; color: var(--create-dim); margin: 5px 2px 10px; }
  hr { border: 0; border-top: 1px solid var(--create-line); margin: 5px 0; }
  button { width: 100%; text-align: left; }
</style>
