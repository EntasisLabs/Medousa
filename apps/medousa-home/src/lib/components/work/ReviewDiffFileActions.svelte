<script lang="ts">
  import { FileCode2, RotateCcw } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  let { onOpenFile, onRestore, busy = false }: {
    onOpenFile: () => void | Promise<void>; onRestore?: () => void | Promise<void>; busy?: boolean;
  } = $props();
  let open = $state(false);
</script>
<div class="file-actions">
  <OverflowMenu bind:open label="File actions" title="File actions" panelClass="w-56 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl">
    <button type="button" role="menuitem" onclick={() => { open = false; void onOpenFile(); }}><FileCode2 size={13}/>Open in Code</button>
    {#if onRestore}<button type="button" role="menuitem" class="restore" disabled={busy} onclick={() => { open = false; void onRestore?.(); }}><RotateCcw size={13}/>Restore before this change…</button>{/if}
  </OverflowMenu>
</div>
<style>
  button { display:flex; width:100%; align-items:center; gap:.45rem; padding:.4rem .55rem; border:0; border-radius:.35rem; background:transparent; color:rgb(var(--theme-text-secondary)); font-size:.75rem; text-align:left; }
  button:hover { background:rgb(var(--theme-text)/.06); }
  button:disabled { opacity:.4; }
  .restore { color:rgb(var(--theme-warning)); }
</style>
