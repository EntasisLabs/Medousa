<script lang="ts">
  import type { LineStyle, LineStylePatch } from "$lib/create/creativeLine";
  let { value, disabled = false, onchange }: { value: LineStyle; disabled?: boolean; onchange: (patch: LineStylePatch) => void } = $props();
  function thickness(n: number) { if (Number.isFinite(n)) onchange({ stroke_width: Math.max(.5, Math.min(100, n)) }); }
</script>

<div class="create-line-controls" role="group" aria-label="Line style">
  <label>Thickness<input aria-label="Line thickness" type="number" min="0.5" max="100" step="0.5" value={value.stroke_width} {disabled}
    oninput={(e) => { const n = e.currentTarget.valueAsNumber; if (n >= .5 && n <= 100) thickness(n); }} onchange={(e) => thickness(e.currentTarget.valueAsNumber)}/></label>
  <label>Stroke<select aria-label="Line pattern" {disabled} value={value.dashed ? "dashed" : "solid"} onchange={(e) => onchange({ dashed: e.currentTarget.value === "dashed" })}>
    <option value="solid">Solid</option><option value="dashed">Dashed</option>
  </select></label>
  <div class="heads">
    <label>Start<select aria-label="Start arrowhead" {disabled} value={value.start_arrow ? "arrow" : "none"} onchange={(e) => onchange({ start_arrow: e.currentTarget.value === "arrow" })}><option value="none">None</option><option value="arrow">Arrow</option></select></label>
    <label>End<select aria-label="End arrowhead" {disabled} value={value.end_arrow ? "arrow" : "none"} onchange={(e) => onchange({ end_arrow: e.currentTarget.value === "arrow" })}><option value="none">None</option><option value="arrow">Arrow</option></select></label>
  </div>
</div>

<style>
  .create-line-controls { display: grid; gap: 8px; font-size: 11px; }
  label { display: grid; gap: 5px; color: var(--create-dim); min-width: 0; }
  select, input { width: 100%; min-width: 0; box-sizing: border-box; padding: 6px; border: 1px solid var(--create-line); border-radius: 5px; color: var(--create-text); background: var(--create-soft); font: inherit; }
  .heads { display: grid; grid-template-columns: 1fr 1fr; gap: 5px; }
</style>
