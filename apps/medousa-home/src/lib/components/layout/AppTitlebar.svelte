<script lang="ts">
  import {
    ArrowLeft,
    ArrowRight,
    Columns2,
    ExternalLink,
    PanelLeft,
    PanelLeftClose,
    Plus,
  } from "@lucide/svelte";
  import ShellTabStrip from "$lib/components/shell/ShellTabStrip.svelte";
  import NewTabMenu from "$lib/components/layout/NewTabMenu.svelte";
  import WindowControls from "$lib/components/layout/WindowControls.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { titlebarMode, usesUnifiedTitlebar } from "$lib/platform";
  import { isTauri, showChatPopout } from "$lib/window";

  const mode = $derived(titlebarMode());
  const show = $derived(usesUnifiedTitlebar());
  const groupId = $derived(shellTabs.activeGroupId);
  const railExpanded = $derived(layout.shellSidebarExpanded);
  const railWidth = $derived(layout.shellSidebarWidth);
  const canNavBack = $derived(
    shellTabs.canGoNavBack || layout.shellSidebarMode === "view",
  );
  const canNavForward = $derived(shellTabs.canGoNavForward);
  const showChatPopoutBtn = $derived(
    isTauri() && shellTabs.activeTab?.kind === "chat",
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
    if (shellTabs.canGoNavBack) {
      void shellTabs.goNavBack();
      return;
    }
    if (layout.shellSidebarMode === "view") {
      layout.shellSidebarBackToNav();
    }
  }

  function goNavForward() {
    void shellTabs.goNavForward();
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
        <div class="app-titlebar-rail-nav" role="group" aria-label="Tab history">
          <button
            type="button"
            class="app-titlebar-btn"
            title="Go back"
            aria-label="Go back"
            disabled={!canNavBack}
            onclick={goNavBack}
          >
            <ArrowLeft size={14} strokeWidth={1.85} />
          </button>
          <button
            type="button"
            class="app-titlebar-btn"
            title="Go forward"
            aria-label="Go forward"
            disabled={!canNavForward}
            onclick={goNavForward}
          >
            <ArrowRight size={14} strokeWidth={1.85} />
          </button>
        </div>
      {/if}
    </div>

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
    align-items: stretch;
    gap: 0;
    padding-left: 0;
    padding-right: 6px;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.18);
    background: rgb(var(--color-surface-950));
    user-select: none;
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
    margin-left: 1px;
    padding-left: 2px;
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
