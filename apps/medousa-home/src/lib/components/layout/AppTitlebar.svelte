<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    ExternalLink,
    PanelLeft,
    PanelLeftOpen,
    Plus,
  } from "@lucide/svelte";
  import ShellTabNotch from "$lib/components/shell/ShellTabNotch.svelte";
  import NewTabMenu from "$lib/components/layout/NewTabMenu.svelte";
  import WindowControls from "$lib/components/layout/WindowControls.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { titlebarMode, usesUnifiedTitlebar } from "$lib/platform";
  import { isTauri, showBrowser, showChatPopout } from "$lib/window";

  const mode = $derived(titlebarMode());
  const show = $derived(usesUnifiedTitlebar());
  const railExpanded = $derived(layout.shellSidebarExpanded);
  const railWidth = $derived(layout.shellSidebarWidth);
  const canNavBack = $derived(layout.canGoRailViewBack);
  const canNavForward = $derived(layout.canGoRailViewForward);
  const showChatPopoutBtn = $derived(
    isTauri() && shellTabs.activeTab?.kind === "chat",
  );
  const showWebPopoutBtn = $derived(
    isTauri() && shellTabs.activeTab?.kind === "web",
  );

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
        title={railExpanded ? "Collapse navigation rail" : "Expand navigation rail"}
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
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
      <ShellTabNotch />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="app-titlebar-drag"
        data-tauri-drag-region
        ondblclick={onDragDblClick}
      ></div>
    </div>

    <div class="app-titlebar-actions shrink-0">
      <NewTabMenu>
        <Plus size={16} />
      </NewTabMenu>
      <!-- Always reserve the pop-out slot so the centered notch doesn't shift. -->
      {#if showChatPopoutBtn}
        <button
          type="button"
          class="app-titlebar-btn"
          title="Pop out chat"
          aria-label="Pop out chat"
          onclick={() => void showChatPopout()}
        >
          <ExternalLink size={16} />
        </button>
      {:else if showWebPopoutBtn}
        <button
          type="button"
          class="app-titlebar-btn"
          title="Open web window"
          aria-label="Open web window"
          onclick={() => void showBrowser()}
        >
          <ExternalLink size={16} />
        </button>
      {:else}
        <span class="app-titlebar-btn app-titlebar-btn--ghost" aria-hidden="true"></span>
      {/if}
    </div>

    <WindowControls />
  </header>
{/if}

<style>
  /*
   * y on trafficLightPosition moves lights DOWN (I had been lowering y — oops).
   * Bar height centers controls on light midline: y18 + 6 ≈ 24 → ~36–40px bar.
   *
   * --titlebar-system-chrome mirrors Mac traffic lights ↔ Win/Linux controls so
   * the centered notch stays optically stable across platforms.
   */
  .app-titlebar {
    --titlebar-height: 40px;
    --titlebar-system-chrome: 86px;
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
    /* Same left footprint as WindowControls on the right (Mac lights / Win spacer). */
    padding-left: var(--titlebar-system-chrome);
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
    margin: 0;
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
    gap: 4px;
    margin-right: 2px;
    padding-left: 4px;
  }

  .app-titlebar-btn--ghost {
    visibility: hidden;
    pointer-events: none;
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

  /* Slightly larger chrome glyphs than the old 13–14px set. */
  .app-titlebar :global(svg.lucide) {
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
