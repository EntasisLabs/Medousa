<script lang="ts">
  import { LME_AUTOMATIONS_PRIMARY_MODES, type LmeExplorerFamily } from "$lib/utils/lmeExplorerModes";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  let { family = "automations" }: { family?: LmeExplorerFamily } = $props();
</script>
{#if family === "automations"}
  <div class="flex shrink-0 flex-wrap gap-1 border-b border-surface-500/25 p-2" aria-label="Automation resources">
    <button class="rounded px-2 py-1 text-xs text-content-secondary hover:bg-surface-800" onclick={() => { shellTabs.openSurface("automations"); layout.setShellSidebarMode("nav"); }}>Run list</button>
    {#each LME_AUTOMATIONS_PRIMARY_MODES as entry (entry.id)}
      <button class="rounded px-2 py-1 text-xs hover:bg-surface-800" class:bg-surface-700={lmeWorkspace.explorerMode === entry.id} onclick={() => lmeWorkspace.setExplorerMode(entry.id)}>{entry.label}</button>
    {/each}
  </div>
{/if}
