<script lang="ts">
  import { Check, ChevronDown, Grid2X2 } from "@lucide/svelte";
  import type { LayoutPreferences } from "$lib/create/creativeLayout";
  let { settings = $bindable(), open = $bindable(false), onopen = () => {}, onfit, onselection, hasSelection }: { settings: LayoutPreferences; open?: boolean; onopen?: () => void; onfit: () => void; onselection: () => void; hasSelection: boolean } = $props();
  let menu = $state<HTMLDetailsElement>();
  function outside(event: PointerEvent) { if (menu?.open && event.target instanceof Node && !menu.contains(event.target)) open = false; }
  function escape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    const details = (event.target as Element).closest("details");
    if (details) { event.stopPropagation(); open = false; details.querySelector("summary")?.focus(); }
  }
</script>

<svelte:window onpointerdowncapture={outside}/>

<details bind:this={menu} bind:open class="layout-view" ontoggle={e => { if (e.currentTarget.open) onopen(); }}>
  <summary onkeydown={escape} aria-label="Canvas view options" aria-expanded={open}><Grid2X2 size={13}/>View<ChevronDown size={11}/></summary>
  <div class="layout-view-menu" role="dialog" aria-label="Canvas view settings" tabindex="-1" onkeydown={escape}>
    <label>Grid<select aria-label="Grid appearance" value={settings.grid} onchange={e => settings = { ...settings, grid: e.currentTarget.value as LayoutPreferences["grid"] }}><option value="none">Off</option><option value="dots">Dots</option><option value="lines">Lines</option></select></label>
    <label>Grid spacing<select aria-label="Grid spacing" value={settings.gridSize} onchange={e => settings = { ...settings, gridSize: Number(e.currentTarget.value) }}>{#each [8, 16, 32, 64] as value}<option {value}>{value} px</option>{/each}</select></label>
    <button class="check-row" type="button" role="checkbox" aria-checked={settings.snapGrid} onclick={() => settings = { ...settings, snapGrid: !settings.snapGrid }}>Snap to grid<span>{#if settings.snapGrid}<Check size={13}/>{/if}</span></button>
    <button class="check-row" type="button" role="checkbox" aria-checked={settings.smartGuides} onclick={() => settings = { ...settings, smartGuides: !settings.smartGuides }}>Smart guides &amp; snapping<span>{#if settings.smartGuides}<Check size={13}/>{/if}</span></button>
    <p>Hold Ctrl while dragging to ignore snapping.</p>
    <hr/>
    <button type="button" onclick={() => { onfit(); open = false; }}>Fit canvas</button>
    <button type="button" disabled={!hasSelection} onclick={() => { onselection(); open = false; }}>Zoom to selection</button>
  </div>
</details>

<style>
  .layout-view { pointer-events: auto; }
  summary { display: flex; align-items: center; gap: 4px; padding: 5px; border-radius: 5px; cursor: pointer; list-style: none; }
  summary:hover, summary[aria-expanded="true"] { background: var(--create-soft); color: var(--create-text); }
  summary::-webkit-details-marker { display: none; }
  .layout-view-menu { position: absolute; right: 0; top: calc(100% + 6px); width: min(235px, calc(100vw - 36px)); padding: 10px; border: 1px solid var(--create-line); background: var(--create-raised); border-radius: 9px; box-shadow: 0 6px 20px #0002; color: var(--create-text); }
  label { display: flex; align-items: center; gap: 8px; padding: 7px 2px; }
  select { margin-left: auto; background: var(--create-soft); color: inherit; border: 0; border-radius: 5px; padding: 5px; }
  .check-row { display: flex; align-items: center; justify-content: space-between; padding: 8px 4px; border-radius: 5px; }
  .check-row:hover { background: var(--create-soft); }
  .check-row span { width: 15px; height: 15px; display: grid; place-items: center; color: var(--create-selection); }
  p { font-size: 10px; color: var(--create-dim); margin: 5px 2px 10px; }
  hr { border: 0; border-top: 1px solid var(--create-line); margin: 5px 0; }
  button { width: 100%; text-align: left; }
</style>
