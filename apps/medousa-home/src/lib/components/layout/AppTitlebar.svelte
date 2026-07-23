<script lang="ts">
  import { Columns2, PanelLeft, PanelLeftClose, Plus } from "@lucide/svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import WindowControls from "$lib/components/layout/WindowControls.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { titlebarMode, usesUnifiedTitlebar } from "$lib/platform";

  const mode = $derived(titlebarMode());
  const show = $derived(usesUnifiedTitlebar());
  const groupId = $derived(shellTabs.activeGroupId);
  const railExpanded = $derived(layout.shellSidebarExpanded);

  function toggleRail() {
    if (railExpanded) {
      layout.setShellSidebarExpanded(false);
      void environment.patchShellChromeDesktop({ navStyle: "compact" }).catch(() => {});
    } else {
      layout.openShellSidebarView(layout.desktopSurface);
      void environment.patchShellChromeDesktop({ navStyle: "rail" }).catch(() => {});
    }
  }

  function newTab() {
    shellTabs.openSurface("library", { activate: true });
  }

  function splitRight() {
    shellTabs.splitActive("right");
  }

  async function onDragDblClick(event: MouseEvent) {
    if (mode !== "custom-winlinux") return;
    if (event.detail !== 2) return;
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().toggleMaximize();
    } catch {
      /* ignore */
    }
  }
</script>

{#if show}
  <header
    class="app-titlebar"
    class:app-titlebar--mac={mode === "overlay-mac"}
    class:app-titlebar--winlinux={mode === "custom-winlinux"}
    data-debug-label="app-titlebar"
    aria-label="Window title bar"
  >
    <button
      type="button"
      class="app-titlebar-btn"
      title={railExpanded ? "Hide sidebar" : "Show sidebar"}
      aria-label={railExpanded ? "Hide sidebar" : "Show sidebar"}
      aria-pressed={railExpanded}
      onclick={toggleRail}
    >
      {#if railExpanded}
        <PanelLeftClose size={14} strokeWidth={1.75} />
      {:else}
        <PanelLeft size={14} strokeWidth={1.75} />
      {/if}
    </button>

    <div class="app-titlebar-tabs min-w-0 flex-1">
      <ShellTabStrip {groupId} variant="titlebar" />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
    </div>

    <div class="app-titlebar-actions shrink-0">
      <button
        type="button"
        class="app-titlebar-btn"
        title="New tab"
        aria-label="New tab"
        onclick={newTab}
      >
        <Plus size={14} strokeWidth={2} />
      </button>
      <button
        type="button"
        class="app-titlebar-btn"
        title="Split pane right"
        aria-label="Split pane right"
        onclick={splitRight}
      >
        <Columns2 size={14} strokeWidth={1.75} />
      </button>
    </div>

    <WindowControls />
  </header>
{/if}

<style>
  /*
   * y on trafficLightPosition moves lights DOWN (I had been lowering y — oops).
   * Bar height centers controls on light midline: y18 + 6 ≈ 24 → ~36–40px bar.
   */
  .app-titlebar {
    --titlebar-height: 40px;
    --titlebar-left-inset: 0px;
    display: flex;
    height: var(--titlebar-height);
    flex-shrink: 0;
    align-items: center;
    gap: 1px;
    padding-left: var(--titlebar-left-inset);
    padding-right: 6px;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.18);
    background: rgb(var(--color-surface-950));
    user-select: none;
  }

  .app-titlebar--mac {
    --titlebar-left-inset: 80px;
  }

  .app-titlebar--winlinux {
    padding-left: 6px;
  }

  .app-titlebar-tabs {
    display: flex;
    min-width: 0;
    height: 100%;
    align-items: center;
    margin-left: 1px;
  }

  .app-titlebar-drag {
    flex: 1 1 auto;
    align-self: stretch;
    min-width: 1.25rem;
  }

  .app-titlebar-actions {
    display: inline-flex;
    align-items: center;
    gap: 0;
  }

  .app-titlebar-btn {
    display: inline-flex;
    width: 24px;
    height: 24px;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: rgb(var(--color-surface-400));
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .app-titlebar-btn:hover {
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-surface-100));
  }
</style>
