<script lang="ts">
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
  .text-emphasis button { flex: 1; font: 14px serif; }
</style>
