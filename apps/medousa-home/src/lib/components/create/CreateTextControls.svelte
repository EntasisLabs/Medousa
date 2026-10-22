<script lang="ts">
  import { AlignLeft, AlignCenter, AlignRight } from "@lucide/svelte";
  import { canvasFonts, defaultTextStyle, type TextStylePatch } from "$lib/create/creativeText";
  let { value = {}, disabled = false, onchange, onleave = () => {} }: {
    value?: TextStylePatch;
    disabled?: boolean;
    onchange: (patch: TextStylePatch) => void;
    onleave?: () => void;
  } = $props();
  const family = $derived(value.font_family ?? defaultTextStyle.font_family);
  const size = $derived(value.font_size ?? defaultTextStyle.font_size);
  function changeSize(value: number) {
    if (Number.isFinite(value)) onchange({ font_size: Math.max(4, Math.min(512, value)) });
  }
</script>

<div class="create-text-controls" data-create-text-controls role="group" aria-label="Text style"
  onfocusout={(e) => {
    if (!(e.relatedTarget instanceof Element && e.relatedTarget.closest("[data-create-text-controls], .create-inline-text"))) onleave();
  }}>
  <label>Font
    <select aria-label="Font family" {disabled} value={family} onchange={(e) => onchange({ font_family: e.currentTarget.value })}>
      {#if !canvasFonts.some(font => font.value === family)}<option value={family}>{family}</option>{/if}
      {#each canvasFonts as font}<option value={font.value}>{font.label}</option>{/each}
    </select>
  </label>
  <label>Size
    <div class="text-size-row">
      <button type="button" aria-label="Decrease font size" disabled={disabled || size <= 4} onclick={() => changeSize(size - 2)}>−</button>
      <input aria-label="Font size" type="number" min="4" max="512" step="1" {disabled} value={size}
        oninput={(e) => { const n = e.currentTarget.valueAsNumber; if (Number.isFinite(n) && n >= 4 && n <= 512) changeSize(n); }}
        onchange={(e) => changeSize(e.currentTarget.valueAsNumber)}
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter" || e.key === "Escape") { e.preventDefault(); e.currentTarget.blur(); }
        }}/>
      <button type="button" aria-label="Increase font size" disabled={disabled || size >= 512} onclick={() => changeSize(size + 2)}>+</button>
    </div>
  </label>
  <div class="text-emphasis" role="group" aria-label="Emphasis">
    <button type="button" aria-label="Bold" aria-pressed={(value.font_weight ?? 400) >= 600} {disabled}
      onclick={() => onchange({ font_weight: (value.font_weight ?? 400) >= 600 ? 400 : 700 })}><b>B</b></button>
    <button type="button" aria-label="Italic" aria-pressed={value.font_style === "italic"} {disabled}
      onclick={() => onchange({ font_style: value.font_style === "italic" ? "normal" : "italic" })}><i>I</i></button>
  </div>
  <div class="text-emphasis" role="group" aria-label="Text alignment">
    {#each [{ value: "left", icon: AlignLeft }, { value: "center", icon: AlignCenter }, { value: "right", icon: AlignRight }] as alignment}
      <button type="button" aria-label={`Align text ${alignment.value}`} aria-pressed={(value.text_align ?? "left") === alignment.value} {disabled} onclick={() => onchange({ text_align: alignment.value as "left" | "center" | "right" })}><alignment.icon size={14}/></button>
    {/each}
  </div>
  <label>Text box<select aria-label="Text sizing" {disabled} value={value.text_width ? "fixed" : "auto"} onchange={e => onchange({ text_width: e.currentTarget.value === "auto" ? null : 240 })}><option value="auto">Auto width</option><option value="fixed">Wrap to width</option></select></label>
  {#if value.text_width}<label>Box width<input aria-label="Text box width" type="number" min="4" max="10000000" value={value.text_width} {disabled} oninput={e => { if (Number.isFinite(e.currentTarget.valueAsNumber)) onchange({ text_width: Math.max(4, Math.min(1e7, e.currentTarget.valueAsNumber)) }); }}/></label>{/if}
  <label>Line spacing<input aria-label="Line spacing" type="number" min=".8" max="4" step=".1" value={value.line_height ?? 1.3} {disabled} oninput={e => { if (Number.isFinite(e.currentTarget.valueAsNumber)) onchange({ line_height: Math.max(.8, Math.min(4, e.currentTarget.valueAsNumber)) }); }}/></label>
</div>

<style>
  .create-text-controls { display: grid; gap: 8px; font-size: 11px; }
  label { display: grid; gap: 5px; color: var(--create-dim); }
  select, input { min-width: 0; width: 100%; box-sizing: border-box; border: 1px solid var(--create-line); border-radius: 5px; background: var(--create-soft); color: var(--create-text); font: inherit; padding: 6px; }
  .text-size-row { display: grid; grid-template-columns: 26px minmax(0, 1fr) 26px; gap: 3px; }
  .text-size-row input { text-align: center; appearance: textfield; }
  input::-webkit-inner-spin-button { appearance: none; }
  button { color: var(--create-text); border-radius: 5px; min-height: 28px; background: var(--create-soft); }
  button:hover, button[aria-pressed="true"] { background: var(--create-line); }
  button:disabled { opacity: .4; }
  .text-emphasis { display: flex; gap: 4px; }
  .text-emphasis button { display: grid; place-items: center; flex: 1; font: 14px serif; }
</style>
