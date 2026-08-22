<script lang="ts">
  import ShellTabNotchMiniLayout from "$lib/components/shell/ShellTabNotchMiniLayout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { Plus, Search } from "@lucide/svelte";
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
  <div class="shell-tab-notch-drawer-stage">
    <ShellTabNotchMiniLayout node={shellTabs.splitRoot} {onTabSettled} />
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
