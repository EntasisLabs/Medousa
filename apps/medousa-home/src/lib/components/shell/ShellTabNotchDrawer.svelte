<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  interface Props {
    onTabSettled?: (info: { tabId: string; didMove: boolean }) => void;
    /** Bind the positioned sheet root for placeToolbarPopover. */
    sheetEl?: HTMLDivElement | null;
  }

  let {
    onTabSettled,
    sheetEl = $bindable<HTMLDivElement | null>(null),
  }: Props = $props();

  const paneCount = $derived(shellTabs.paneCount);
</script>

<div
  bind:this={sheetEl}
  class="shell-tab-notch-drawer"
  class:shell-tab-notch-drawer--single={paneCount <= 1}
  role="dialog"
  aria-label="Panes"
  onclick={(event) => event.stopPropagation()}
>
  <div class="shell-tab-notch-drawer-stage">
    <ShellTabNotchMiniLayout node={shellTabs.splitRoot} {onTabSettled} />
  </div>
</div>
