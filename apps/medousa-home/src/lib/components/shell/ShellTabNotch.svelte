<script lang="ts">
  import "$lib/styles/shell-tabs.postcss";
  import NewTabMenu from "$lib/components/layout/NewTabMenu.svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { Plus } from "@lucide/svelte";

  const groupId = $derived(shellTabs.activeGroupId);
  const activeTabs = $derived(shellTabs.tabsForGroup(groupId));
</script>

<div class="shell-titlebar-tabs" data-debug-label="shell-titlebar-tabs">
  {#if activeTabs.length > 0}
    <ShellTabStrip {groupId} variant="titlebar" />
  {:else}
    <span class="shell-titlebar-tabs-empty">No tabs</span>
  {/if}
  <NewTabMenu>
    <Plus size={15} strokeWidth={1.8} />
  </NewTabMenu>
</div>

<style>
  .shell-titlebar-tabs {
    display: flex;
    min-width: 0;
    max-width: min(42rem, 58vw);
    flex: 0 1 auto;
    align-items: center;
    gap: 0.2rem;
  }

  .shell-titlebar-tabs :global(.shell-tab-strip--titlebar) {
    min-width: 0;
    max-width: 100%;
  }

  .shell-titlebar-tabs :global(.shell-tab-chip) {
    max-width: 11rem;
  }

  .shell-titlebar-tabs :global(.shell-tab-chip--active) {
    background: rgb(var(--color-surface-700) / 0.72);
    color: rgb(var(--color-surface-50));
  }

  .shell-titlebar-tabs :global(.shell-tab-chip--idle) {
    color: rgb(var(--theme-text-tertiary));
  }

  .shell-titlebar-tabs-empty {
    padding: 0 0.35rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  .shell-titlebar-tabs :global(.app-titlebar-btn) {
    display: inline-flex;
    width: 24px;
    height: 24px;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
  }

  .shell-titlebar-tabs :global(.app-titlebar-btn:hover:not(:disabled)) {
    background: rgb(var(--color-surface-800) / 0.65);
    color: rgb(var(--color-surface-100));
  }
</style>
