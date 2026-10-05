<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { Plus, Search } from "@lucide/svelte";
  import { tick } from "svelte";
  import type { SplitNode } from "$lib/types/shellTabs";

  interface Props {
    onTabSettled?: (info: { tabId: string; didMove: boolean }) => void;
    onSearch?: () => void;
    /** Bind the positioned sheet root for placeToolbarPopover. */
    sheetEl?: HTMLDivElement | null;
  }

  let {
    onTabSettled,
    onSearch,
    sheetEl = $bindable<HTMLDivElement | null>(null),
  }: Props = $props();

  const paneCount = $derived(shellTabs.paneCount);
  // Keep each pane readable even in nested/uneven splits. The stage scrolls
  // when the spatial map needs more room than the viewport can offer.
  function mapHeight(node: SplitNode): number {
    if (node.type === "group") return 144;
    const a = mapHeight(node.a);
    const b = mapHeight(node.b);
    return node.direction === "column"
      ? Math.max(a, b)
      : Math.max(a / node.ratio, b / (1 - node.ratio)) + 6;
  }
  const paneMapHeight = $derived(mapHeight(shellTabs.splitRoot));
  let renamingDesktop = $state(false);
  let renameDraft = $state("");
  let renameInputEl = $state<HTMLInputElement | null>(null);

  async function beginDesktopRename() {
    renamingDesktop = true;
    renameDraft = shellTabs.activeDesktopName;
    await tick();
    renameInputEl?.focus();
    renameInputEl?.select();
  }

  function commitDesktopRename() {
    if (!renamingDesktop) return;
    const next = renameDraft.trim();
    renamingDesktop = false;
    if (next) shellTabs.renameDesktop(shellTabs.activeDesktopId, next);
  }

  function onRenameKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitDesktopRename();
    } else if (event.key === "Escape") {
      event.preventDefault();
      renamingDesktop = false;
    }
  }
</script>

<div
  bind:this={sheetEl}
  class="shell-tab-notch-drawer"
  role="dialog"
  tabindex="-1"
  aria-label="Panes"
  onclick={(event) => event.stopPropagation()}
  onkeydown={(event) => event.stopPropagation()}
>
  <div class="shell-tab-notch-drawer-stage">
    <div class="shell-tab-notch-map" style:height="{paneMapHeight}px">
      <ShellTabNotchMiniLayout node={shellTabs.splitRoot} {onTabSettled} />
    </div>
  </div>

  <footer class="shell-tab-notch-drawer-footer">
    <div class="shell-tab-notch-drawer-desktop-copy">
      {#if renamingDesktop}
        <input
          bind:this={renameInputEl}
          bind:value={renameDraft}
          class="shell-tab-notch-drawer-desktop-name-input"
          aria-label="Rename desktop"
          maxlength={32}
          spellcheck="false"
          onkeydown={onRenameKeydown}
          onblur={commitDesktopRename}
        />
      {:else}
        <button
          type="button"
          class="shell-tab-notch-drawer-desktop-name"
          title="Double-click to rename"
          ondblclick={(event) => {
            event.preventDefault();
            void beginDesktopRename();
          }}
        >{shellTabs.activeDesktopName}</button>
      {/if}
      <span>{paneCount} pane{paneCount === 1 ? "" : "s"}</span>
    </div>
    <div class="shell-tab-notch-drawer-desktop-actions" role="group" aria-label="Desktop actions">
      <button
        type="button"
        class="shell-tab-notch-drawer-quiet-action"
        title="Search open tabs"
        aria-label="Search open tabs"
        onclick={onSearch}
      >
        <Search size={14} strokeWidth={1.8} />
      </button>
      <span class="shell-tab-notch-drawer-footer-divider" aria-hidden="true"></span>
      {#each shellTabs.desktops as desktop, index (desktop.id)}
        <button
          type="button"
          class:active={desktop.id === shellTabs.activeDesktopId}
          title={desktop.name}
          aria-label="Switch to {desktop.name}"
          aria-current={desktop.id === shellTabs.activeDesktopId ? "true" : undefined}
          onclick={() => void shellTabs.switchDesktop(desktop.id)}
        >{index + 1}</button>
      {/each}
      <button
        type="button"
        title="Create desktop"
        aria-label="Create desktop"
        disabled={!shellTabs.canCreateDesktop}
        onclick={() => shellTabs.createDesktop()}
      ><Plus size={14} strokeWidth={1.8} /></button>
    </div>
  </footer>
</div>
