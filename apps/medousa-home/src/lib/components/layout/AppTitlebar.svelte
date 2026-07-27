<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    Columns2,
    ExternalLink,
    PanelLeft,
    PanelLeftClose,
    Plus,
    Rows2,
    SquareX,
  } from "@lucide/svelte";
  import ShellTabNotch from "$lib/components/shell/ShellTabNotch.svelte";
  import NewTabMenu from "$lib/components/layout/NewTabMenu.svelte";
  import WindowControls from "$lib/components/layout/WindowControls.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { MAX_SHELL_PANES } from "$lib/types/shellTabs";
  import { titlebarMode, usesUnifiedTitlebar } from "$lib/platform";
  import { isTauri, showBrowser, showChatPopout } from "$lib/window";

  const mode = $derived(titlebarMode());
  const show = $derived(usesUnifiedTitlebar());
  const railExpanded = $derived(layout.shellSidebarExpanded);
  const railWidth = $derived(layout.shellSidebarWidth);
  const canNavBack = $derived(layout.canGoRailViewBack);
  const canNavForward = $derived(layout.canGoRailViewForward);
  const canSplit = $derived(shellTabs.paneCount < MAX_SHELL_PANES);
  const canMergePane = $derived(shellTabs.paneCount > 1);
  const showChatPopoutBtn = $derived(
    isTauri() && shellTabs.activeTab?.kind === "chat",
  );
  const showWebPopoutBtn = $derived(
    isTauri() && shellTabs.activeTab?.kind === "web",
  );
  /** Native webview owns the pane chrome — titlebar notch overlaps / misrenders. */
  const showShellTabNotch = $derived(shellTabs.activeTab?.kind !== "web");

  function toggleRail() {
    if (railExpanded) {
      layout.setShellSidebarExpanded(false);
      void environment.patchShellChromeDesktop({ navStyle: "compact" }).catch(() => {});
    } else {
      layout.openShellSidebarView(layout.desktopSurface);
      void environment.patchShellChromeDesktop({ navStyle: "rail" }).catch(() => {});
    }
  }

  function goNavBack() {
    layout.goRailViewBack();
  }

  function goNavForward() {
    layout.goRailViewForward();
  }

  function splitRight() {
    shellTabs.splitActive("right");
  }

  function splitDown() {
    shellTabs.splitActive("down");
  }

  function closePane() {
    shellTabs.closeActiveGroup();
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
    class:app-titlebar--rail-expanded={railExpanded}
    data-debug-label="app-titlebar"
    aria-label="Window title bar"
  >
    <div
      class="app-titlebar-rail-slot"
      class:app-titlebar-rail-slot--expanded={railExpanded}
      style={railExpanded ? `width: ${railWidth}px` : undefined}
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

      {#if railExpanded}
        <div class="app-titlebar-rail-nav" role="group" aria-label="Side rail history">
          <button
            type="button"
            class="app-titlebar-btn"
            title="Side rail back"
            aria-label="Side rail back"
            disabled={!canNavBack}
            onclick={goNavBack}
          >
            <ArrowLeft size={14} strokeWidth={1.85} />
          </button>
          <button
            type="button"
            class="app-titlebar-btn"
            title="Side rail forward"
            aria-label="Side rail forward"
            disabled={!canNavForward}
            onclick={goNavForward}
          >
            <ArrowRight size={14} strokeWidth={1.85} />
          </button>
        </div>
      {/if}
    </div>

    <div class="app-titlebar-tabs min-w-0 flex-1">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
      {#if showShellTabNotch}
        <ShellTabNotch />
      {/if}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
    </div>

    <div class="app-titlebar-actions shrink-0">
      <NewTabMenu>
        <Plus size={14} strokeWidth={2} />
      </NewTabMenu>
      {#if showChatPopoutBtn}
        <button
          type="button"
          class="app-titlebar-btn"
          title="Pop out chat"
          aria-label="Pop out chat"
          onclick={() => void showChatPopout()}
        >
          <ExternalLink size={14} strokeWidth={1.75} />
        </button>
      {/if}
      {#if showWebPopoutBtn}
        <button
          type="button"
          class="app-titlebar-btn"
          title="Open web window"
          aria-label="Open web window"
          onclick={() => void showBrowser()}
        >
          <ExternalLink size={14} strokeWidth={1.75} />
        </button>
      {/if}
      <button
        type="button"
        class="app-titlebar-btn"
        title="Split pane right"
        aria-label="Split pane right"
        disabled={!canSplit}
        onclick={splitRight}
      >
        <Columns2 size={14} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="app-titlebar-btn"
        title="Split pane down"
        aria-label="Split pane down"
        disabled={!canSplit}
        onclick={splitDown}
      >
        <Rows2 size={14} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="app-titlebar-btn"
        title="Close pane · merge tabs"
        aria-label="Close pane and merge tabs"
        disabled={!canMergePane}
        onclick={closePane}
      >
        <SquareX size={14} strokeWidth={1.75} />
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
    align-items: stretch;
    gap: 0;
    padding-left: 0;
    padding-right: 6px;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.18);
    background: rgb(var(--color-surface-950));
    user-select: none;
  }

  /* Keep notch above the fused drawer while open (drawer lives in BodyPortal). */
  :global(.app-titlebar.app-titlebar--notch-open) {
    position: relative;
    z-index: 146;
  }

  .app-titlebar--mac {
    --titlebar-left-inset: 80px;
  }

  .app-titlebar--winlinux {
    padding-left: 0;
  }

  .app-titlebar-rail-slot {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 1px;
    box-sizing: border-box;
    padding-left: max(6px, var(--titlebar-left-inset));
    padding-right: 4px;
    transition:
      background-color 160ms ease,
      width 160ms ease;
  }

  .app-titlebar-rail-slot--expanded {
    justify-content: space-between;
    /* Bridge the titlebar seam so chrome reads continuous with the rail. */
    margin-bottom: -1px;
    padding-bottom: 1px;
    background: rgb(var(--shell-chrome-bg));
  }

  .app-titlebar-rail-nav {
    display: inline-flex;
    align-items: center;
    gap: 0;
    margin-left: auto;
  }

  .app-titlebar-tabs {
    display: flex;
    min-width: 0;
    height: 100%;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    margin-left: 1px;
    padding: 4px 2px;
  }

  .app-titlebar-drag {
    flex: 1 1 0;
    align-self: stretch;
    min-width: 0.75rem;
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

  .app-titlebar-btn:hover:not(:disabled) {
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-surface-100));
  }

  .app-titlebar-btn:disabled {
    opacity: 0.28;
    cursor: default;
  }

  .app-titlebar-rail-slot--expanded .app-titlebar-btn:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.55);
  }
</style>
