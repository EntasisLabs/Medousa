<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES } from "$lib/types/shellTabs";
  import { Columns2, Plus, Rows2, Search, SquareX } from "@lucide/svelte";
  import { tick } from "svelte";

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
  const canSplit = $derived(paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(paneCount > 1);
  const desktopIndex = $derived(
    Math.max(0, shellTabs.desktops.findIndex((desktop) => desktop.id === shellTabs.activeDesktopId)),
  );
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
  class:shell-tab-notch-drawer--single={paneCount <= 1}
  role="dialog"
  tabindex="-1"
  aria-label="Panes"
  onclick={(event) => event.stopPropagation()}
  onkeydown={(event) => event.stopPropagation()}
>
  <section class="shell-tab-notch-drawer-section shell-tab-notch-drawer-customize">
    <div class="shell-tab-notch-drawer-heading">
      <span>Customize view</span>
      <button
        type="button"
        class="shell-tab-notch-drawer-quiet-action"
        title="Search open tabs"
        aria-label="Search open tabs"
        onclick={onSearch}
      >
        <Search size={14} strokeWidth={1.8} />
      </button>
    </div>
    <div class="shell-tab-notch-drawer-actions" role="group" aria-label="Customize active pane">
      <button type="button" disabled={!canSplit} onclick={() => shellTabs.splitActive("right")}>
        <Columns2 size={15} strokeWidth={1.75} />
        <span>Split right</span>
      </button>
      <button type="button" disabled={!canSplit} onclick={() => shellTabs.splitActive("down")}>
        <Rows2 size={15} strokeWidth={1.75} />
        <span>Split down</span>
      </button>
      <button
        type="button"
        disabled={!canMergePane}
        onclick={() => shellTabs.closeActiveGroup()}
      >
        <SquareX size={15} strokeWidth={1.75} />
        <span>Close pane</span>
      </button>
    </div>
  </section>

  <section class="shell-tab-notch-drawer-section shell-tab-notch-drawer-desktop">
    <div class="shell-tab-notch-drawer-desktop-copy">
      <span class="shell-tab-notch-drawer-kicker">Desktop {desktopIndex + 1}</span>
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
    <div class="shell-tab-notch-drawer-desktop-actions">
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
  </section>

  <div class="shell-tab-notch-drawer-stage">
    <ShellTabNotchMiniLayout node={shellTabs.splitRoot} {onTabSettled} />
  </div>
</div>
