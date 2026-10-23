<script lang="ts">
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  import { cornerRadius, defaultShadow, hasCorners, type AppearancePatch } from "$lib/create/creativeAppearance";
  let { node, disabled = false, onchange }: { node: CreativeNode; disabled?: boolean; onchange: (patch: AppearancePatch) => void } = $props();
  const content = $derived(node.content);
  const fill = $derived(content.kind === "shape" ? content.fill : content.kind === "frame" ? content.background : null);
  const shadow = $derived(node.appearance?.shadow);
  const hex = (value: string) => /^#[\da-f]{6}([\da-f]{2})?$/i.test(value) ? value.slice(0, 7) : /^#[\da-f]{3,4}$/i.test(value) ? "#" + [...value.slice(1, 4)].map(c => c + c).join("") : "#393e33";
  function number(event: Event, key: "corner_radius" | "border_width", max: number) {
    const value = (event.target as HTMLInputElement).valueAsNumber;
    if (Number.isFinite(value)) onchange({ [key]: Math.max(0, Math.min(max, value)) });
  }
</script>
<div class="surface-controls" role="group" aria-label="Surface style">
  {#if fill !== null}
    <div class="surface-row"><span>Fill</span><button type="button" aria-label="Toggle fill" aria-pressed={fill !== "transparent"} {disabled} onclick={() => onchange({ fill_color: fill === "transparent" ? "#e7e6df" : "transparent" })}>{fill === "transparent" ? "None" : "Solid"}</button><input type="color" aria-label="Fill color" {disabled} value={hex(fill)} onchange={e => onchange({ fill_color: e.currentTarget.value })}/></div>
  {/if}
  {#if content.kind === "shape"}
    <div class="surface-row"><span>Border</span><input type="number" aria-label="Border width" min="0" max="1000" step="1" value={content.stroke_width} {disabled} oninput={e => number(e, "border_width", 1000)}/><input type="color" aria-label="Border color" {disabled} value={hex(content.stroke)} onchange={e => onchange({ stroke_color: e.currentTarget.value })}/></div>
  {/if}
  {#if hasCorners(node)}<label class="surface-row"><span>Radius</span><input type="number" aria-label="Corner radius" min="0" max={Math.min(node.transform.width, node.transform.height) / 2} step="1" value={Math.round(cornerRadius(node) * 100) / 100} {disabled} oninput={e => number(e, "corner_radius", Math.min(node.transform.width, node.transform.height) / 2)}/></label>{/if}
  <div class="surface-row"><span>Shadow</span><button type="button" aria-label="Toggle shadow" aria-pressed={Boolean(shadow)} {disabled} onclick={() => onchange({ shadow: shadow ? null : { ...defaultShadow } })}>{shadow ? "On" : "Off"}</button></div>
  {#if shadow}
    <div class="shadow-grid">
      {#each ["x", "y", "blur", "opacity"] as key}
        <label>{key === "opacity" ? "Strength" : key === "blur" ? "Blur" : key.toUpperCase()}<input type="number" aria-label={`Shadow ${key}`} min={key === "x" || key === "y" ? -1000 : 0} max={key === "opacity" ? 1 : key === "blur" ? 128 : 1000} step={key === "opacity" ? .05 : 1} value={shadow[key as keyof typeof shadow]} {disabled} oninput={e => {
          const value = e.currentTarget.valueAsNumber, min = key === "x" || key === "y" ? -1000 : 0, max = key === "opacity" ? 1 : key === "blur" ? 128 : 1000;
          if (Number.isFinite(value)) onchange({ shadow: { ...shadow!, [key]: Math.max(min, Math.min(max, value)) } });
        }}/></label>
      {/each}
    </div>
    <label class="surface-row"><span>Shadow color</span><input type="color" aria-label="Shadow color" value={hex(shadow.color)} {disabled} onchange={e => onchange({ shadow: { ...shadow!, color: e.currentTarget.value } })}/></label>
  {/if}
</div>
<style>
  .surface-controls { display: grid; gap: 8px; font-size: 11px; }
  .surface-row { display: flex; gap: 5px; align-items: center; min-height: 26px; }
  span { margin-right: auto; color: var(--create-dim); }
  input, button { min-width: 0; box-sizing: border-box; border: 1px solid var(--create-line); border-radius: 5px; background: var(--create-soft); color: var(--create-text); font: inherit; padding: 5px; }
  input[type="number"] { width: 57px; }
  input[type="color"] { width: 26px; height: 26px; padding: 2px; }
  button { min-width: 42px; }
  button:hover, button[aria-pressed="true"] { background: var(--create-line); }
  :disabled { opacity: .4; }
  .shadow-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .shadow-grid label { display: grid; gap: 3px; color: var(--create-dim); }
  .shadow-grid input { width: 100%; }
</style>
