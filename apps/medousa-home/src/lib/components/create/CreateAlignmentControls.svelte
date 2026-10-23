<script lang="ts">
  import { AlignStartVertical, AlignCenterVertical, AlignEndVertical, AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal, AlignHorizontalSpaceAround, AlignVerticalSpaceAround, ChevronDown } from "@lucide/svelte";
  import type { Alignment } from "$lib/create/creativeSelection";
  import type { AlignmentTarget } from "$lib/create/creativeLayout";
  let { count, hasFrame, disabled, onalign, ondistribute }: { count: number; hasFrame: boolean; disabled: boolean; onalign: (alignment: Alignment, target: AlignmentTarget) => void; ondistribute: (axis: "x" | "y") => void } = $props();
  let target = $state<AlignmentTarget>("selection");
  const effective = $derived(count === 1 ? "frame" : target === "frame" && hasFrame ? "frame" : "selection");
  const actions = [
    { id: "left", label: "Align left", icon: AlignStartVertical }, { id: "center", label: "Align center", icon: AlignCenterVertical }, { id: "right", label: "Align right", icon: AlignEndVertical },
    { id: "top", label: "Align top", icon: AlignStartHorizontal }, { id: "middle", label: "Align middle", icon: AlignCenterHorizontal }, { id: "bottom", label: "Align bottom", icon: AlignEndHorizontal },
  ] as const;
  let menu = $state<HTMLDetailsElement>();
  function outside(event: PointerEvent) { if (menu?.open && event.target instanceof Node && !menu.contains(event.target)) menu.open = false; }
  function escape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    const details = (event.target as Element).closest("details");
    if (details) { event.stopPropagation(); details.open = false; details.querySelector("summary")?.focus(); }
  }
</script>

<svelte:window onpointerdowncapture={outside}/>

{#if count > 1 || hasFrame}
  <details bind:this={menu} class="layout-align">
    <summary onkeydown={escape} aria-label="Align selection">Align<ChevronDown size={11}/></summary>
    <div class="layout-align-menu" role="dialog" aria-label="Object alignment" tabindex="-1" onkeydown={escape}>
      <label>Align to<select aria-label="Alignment reference" value={effective} onchange={(e) => (target = e.currentTarget.value as AlignmentTarget)} disabled={count === 1}>
        {#if count > 1}<option value="selection">Selection</option>{/if}<option value="frame" disabled={!hasFrame}>Frame</option>
      </select></label>
      <div class="alignment-icons" role="group" aria-label="Alignment">
        {#each actions as action}<button type="button" aria-label={action.label} title={action.label} {disabled} onclick={() => onalign(action.id, effective)}><action.icon size={16}/></button>{/each}
      </div>
      {#if count > 1}<div class="spacing" role="group" aria-label="Equal spacing">
        <button type="button" disabled={disabled || count < 3} title="Select three or more objects" onclick={() => ondistribute("x")}><AlignHorizontalSpaceAround size={15}/>Space horizontally</button>
        <button type="button" disabled={disabled || count < 3} title="Select three or more objects" onclick={() => ondistribute("y")}><AlignVerticalSpaceAround size={15}/>Space vertically</button>
      </div>{/if}
    </div>
  </details>
{/if}

<style>
  .layout-align { position: relative; }
  summary { display: flex; align-items: center; gap: 4px; list-style: none; font-size: 11px; padding: 8px 6px; border-radius: 5px; cursor: pointer; }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: var(--create-soft); }
  .layout-align-menu { position: absolute; bottom: calc(100% + 9px); left: 50%; transform: translateX(-50%); width: 214px; padding: 9px; border: 1px solid var(--create-line); border-radius: 8px; background: var(--create-raised); box-shadow: 0 5px 18px #0002; }
  label { display: flex; align-items: center; justify-content: space-between; color: var(--create-dim); font-size: 11px; padding: 2px 0 9px; }
  select { background: var(--create-soft); color: var(--create-text); border: 0; padding: 5px; border-radius: 5px; }
  .alignment-icons { display: grid; grid-template-columns: repeat(6, 1fr); gap: 2px; }
  .spacing { border-top: 1px solid var(--create-line); margin-top: 8px; padding-top: 5px; display: grid; }
  .layout-align-menu .spacing button { display: flex; align-items: center; width: 100%; gap: 8px; height: auto; padding: 8px 5px; font-size: 11px; justify-content: flex-start; white-space: nowrap; }
</style>
