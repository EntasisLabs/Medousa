<script lang="ts">
  import { Columns2, Rows3 } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  let { mode = $bindable<"inline" | "side">("inline"), density = $bindable<"comfortable" | "compact">("compact"), wrap = $bindable(false), onDensityChange, onWrapChange }: {
    mode?: "inline" | "side"; density?: "comfortable" | "compact"; wrap?: boolean;
    onDensityChange?: (value: "comfortable" | "compact") => void;
    onWrapChange?: (value: boolean) => void;
  } = $props();
  let open = $state(false);
  function chooseDensity(value: "comfortable" | "compact") { density = value; onDensityChange?.(value); open = false; }
  function chooseMode(value: "inline" | "side") { mode = value; open = false; }
  function chooseWrap(value: boolean) { wrap = value; onWrapChange?.(value); open = false; }
</script>

<div class="diff-view-options">
  <OverflowMenu
    bind:open
    label="Diff view options"
    title="Diff view options"
    panelClass="w-48 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
  >
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={density === "comfortable"}
      class="secondary-action"
      onclick={() => chooseDensity("comfortable")}
    >Comfortable density</button>
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={density === "compact"}
      class="secondary-action"
      onclick={() => chooseDensity("compact")}
    >Compact density</button>
    <div class="my-1 border-t border-surface-500/25" role="separator"></div>
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={mode === "inline"}
      class="secondary-action"
      onclick={() => chooseMode("inline")}
    >
      <Rows3 size={13} />
      <span>Inline</span>
    </button>
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={mode === "side"}
      class="secondary-action"
      onclick={() => chooseMode("side")}
    >
      <Columns2 size={13} />
      <span>Side by side</span>
    </button>
    <div class="my-1 border-t border-surface-500/25" role="separator"></div>
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={wrap}
      class="secondary-action"
      onclick={() => chooseWrap(!wrap)}
    >Wrap long lines</button>
  </OverflowMenu>
</div>
<style>
  .secondary-action { display:flex; width:100%; align-items:center; gap:.45rem; border:0; border-radius:.4rem; background:transparent; padding:.4rem .55rem; color:rgb(var(--theme-text-secondary)); font-size:.75rem; text-align:left; }
  .secondary-action:hover { background:rgb(var(--color-surface-700)/.55); }
  .secondary-action[aria-checked="true"] { color:rgb(var(--theme-link)); }
</style>
