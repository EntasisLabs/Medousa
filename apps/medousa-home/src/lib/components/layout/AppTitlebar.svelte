<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    PanelLeft,
    PanelLeftOpen,
  } from "@lucide/svelte";
  import ShellTabNotch from "$lib/components/shell/ShellTabNotch.svelte";
  import ShellWorkspaceControl from "$lib/components/shell/ShellWorkspaceControl.svelte";
  import WindowControls from "$lib/components/layout/WindowControls.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { titlebarMode, usesUnifiedTitlebar } from "$lib/platform";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";

  const mode = $derived(titlebarMode());
  const show = $derived(usesUnifiedTitlebar());
  const railExpanded = $derived(layout.shellSidebarExpanded);
  const railWidth = $derived(layout.shellSidebarWidth);
  const canNavBack = $derived(layout.canGoRailViewBack);
  const canNavForward = $derived(layout.canGoRailViewForward);

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
            <ArrowLeft size={16} />
          </button>
          <button
            type="button"
            class="app-titlebar-btn"
            title="Side rail forward"
            aria-label="Side rail forward"
            disabled={!canNavForward}
            onclick={goNavForward}
          >
            <ArrowRight size={16} />
          </button>
        </div>
      {/if}

      <button
        type="button"
        class="app-titlebar-btn"
        title={titleWithShortcut(
          railExpanded ? "Collapse navigation rail" : "Expand navigation rail",
          "toggle-rail",
        )}
        aria-label={railExpanded ? "Collapse navigation rail" : "Expand navigation rail"}
        aria-pressed={railExpanded}
        onclick={toggleRail}
      >
        {#if railExpanded}
          <PanelLeft size={16} />
        {:else}
          <PanelLeftOpen size={16} />
        {/if}
      </button>
    </div>

    <div class="app-titlebar-tabs min-w-0 flex-1">
      <ShellTabNotch />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
      <ShellWorkspaceControl />
    </div>

    <WindowControls />
  </header>
{/if}

<style>
  /*
   * y on trafficLightPosition moves lights DOWN (I had been lowering y — oops).
   * Bar height centers controls on light midline: y18 + 6 ≈ 24 → ~36–40px bar.
   *
   * Native and custom chrome keep their own platform-appropriate footprints.
   */
  .app-titlebar {
    --titlebar-height: 40px;
    display: flex;
    height: var(--titlebar-height);
    flex-shrink: 0;
    align-items: stretch;
    gap: 0;
    padding-left: 0;
    padding-right: 0;
    border-bottom: 0;
    background: rgb(var(--color-surface-950));
    user-select: none;
  }

  /* Keep notch above the fused drawer while open (drawer lives in BodyPortal). */
  :global(.app-titlebar.app-titlebar--notch-open) {
    position: relative;
    z-index: 146;
  }

  .app-titlebar-rail-slot {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 1px;
    box-sizing: border-box;
    padding-left: 4px;
    padding-right: 4px;
    transition:
      background-color 160ms ease,
      width 160ms ease;
  }

  .app-titlebar--mac .app-titlebar-rail-slot {
    /* Clear the native traffic lights without mirroring them on the right. */
    padding-left: 86px;
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
    margin: 0;
  }

  .app-titlebar-tabs {
    display: flex;
    min-width: 0;
    height: 100%;
    align-items: center;
    justify-content: flex-start;
    gap: 0.75rem;
    margin-left: 1px;
    padding: 4px 8px 4px 6px;
  }

  .app-titlebar-drag {
    flex: 1 1 0;
    align-self: stretch;
    min-width: 0.75rem;
  }

  .app-titlebar-btn {
    display: inline-flex;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .app-titlebar-btn :global(svg.lucide) {
    width: 16px;
    height: 16px;
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
