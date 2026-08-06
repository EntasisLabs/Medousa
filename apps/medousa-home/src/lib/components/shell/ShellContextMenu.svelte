<script lang="ts">
  import { onMount } from "svelte";
  import { shellContextMenu } from "$lib/stores/shellContextMenu.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES } from "$lib/types/shellTabs";

  let menuEl = $state<HTMLDivElement | null>(null);

  const target = $derived(shellContextMenu.target);
  const otherDesktops = $derived(
    shellTabs.desktops.filter((desktop) => desktop.id !== shellTabs.activeDesktopId),
  );
  const canSplit = $derived(shellTabs.paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(shellTabs.paneCount > 1);
  const canCreateDesktop = $derived(shellTabs.canCreateDesktop);

  function clampPosition(x: number, y: number): { x: number; y: number } {
    if (typeof window === "undefined") return { x, y };
    const width = menuEl?.offsetWidth ?? 196;
    const height = menuEl?.offsetHeight ?? 140;
    const margin = 8;
    return {
      x: Math.min(Math.max(margin, x), window.innerWidth - width - margin),
      y: Math.min(Math.max(margin, y), window.innerHeight - height - margin),
    };
  }

  const position = $derived(clampPosition(shellContextMenu.x, shellContextMenu.y));

  function moveToDesktop(desktopId: string) {
    if (!target) return;
    if (target.kind === "tab") {
      shellTabs.moveTabToDesktop(target.tabId, desktopId);
    } else {
      shellTabs.movePaneToDesktop(target.groupId, desktopId);
    }
    shellContextMenu.close();
  }

  function moveToNewDesktop() {
    if (!target) return;
    const id = shellTabs.createDesktop(undefined, { activate: false });
    if (!id) return;
    moveToDesktop(id);
  }

  function splitRight() {
    if (!target || target.kind !== "tab" || !canSplit) return;
    shellTabs.splitGroupWithTab(target.groupId, target.tabId, "right");
    shellContextMenu.close();
  }

  function closeTab() {
    if (!target || target.kind !== "tab") return;
    shellTabs.close(target.tabId);
    shellContextMenu.close();
  }

  function closePane() {
    if (!target) return;
    shellTabs.focusGroup(target.groupId);
    shellTabs.closeActiveGroup();
    shellContextMenu.close();
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") shellContextMenu.close();
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!shellContextMenu.open) return;
    if (menuEl?.contains(event.target as Node)) return;
    shellContextMenu.close();
  }

  onMount(() => {
    window.addEventListener("keydown", onWindowKeydown);
    window.addEventListener("pointerdown", onWindowPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onWindowKeydown);
      window.removeEventListener("pointerdown", onWindowPointerDown, true);
    };
  });
</script>

{#if shellContextMenu.open && target}
  <div
    bind:this={menuEl}
    class="vault-context-menu shell-context-menu"
    role="menu"
    style:left="{position.x}px"
    style:top="{position.y}px"
  >
    {#if shellContextMenu.pickingDesktop}
      {#if otherDesktops.length === 0 && !canCreateDesktop}
        <div class="px-3 py-2 text-[12px] text-content-tertiary">No other workspaces</div>
      {:else}
        {#each otherDesktops as desktop (desktop.id)}
          <button
            type="button"
            class="vault-context-menu-item"
            role="menuitem"
            onclick={() => moveToDesktop(desktop.id)}
          >
            {desktop.name}
          </button>
        {/each}
        {#if canCreateDesktop}
          <button
            type="button"
            class="vault-context-menu-item"
            role="menuitem"
            onclick={moveToNewDesktop}
          >
            New workspace
          </button>
        {/if}
      {/if}
      <div class="vault-context-menu-sep" aria-hidden="true"></div>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        onclick={() => {
          shellContextMenu.pickingDesktop = false;
        }}
      >
        Back
      </button>
    {:else if target.kind === "tab"}
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={otherDesktops.length === 0 && !canCreateDesktop}
        onclick={() => shellContextMenu.pickDesktop()}
      >
        Move to workspace…
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={!canSplit}
        onclick={splitRight}
      >
        Split right
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        onclick={closeTab}
      >
        Close tab
      </button>
      {#if canMergePane}
        <button
          type="button"
          class="vault-context-menu-item"
          role="menuitem"
          onclick={closePane}
        >
          Close pane
        </button>
      {/if}
    {:else}
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={otherDesktops.length === 0 && !canCreateDesktop}
        onclick={() => shellContextMenu.pickDesktop()}
      >
        Move pane to workspace…
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={!canSplit}
        onclick={() => {
          shellTabs.focusGroup(target.groupId);
          shellTabs.splitActive("right");
          shellContextMenu.close();
        }}
      >
        Split right
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={!canSplit}
        onclick={() => {
          shellTabs.focusGroup(target.groupId);
          shellTabs.splitActive("down");
          shellContextMenu.close();
        }}
      >
        Split down
      </button>
      <button
        type="button"
        class="vault-context-menu-item"
        role="menuitem"
        disabled={!canMergePane}
        onclick={closePane}
      >
        Close pane
      </button>
    {/if}
  </div>
{/if}
