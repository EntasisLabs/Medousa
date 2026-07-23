<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ArrowLeft, ArrowRight, Square, RefreshCw } from "@lucide/svelte";
  import HumanBrowserTabBar from "$lib/components/browser/HumanBrowserTabBar.svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import BrowserControlHandoff from "$lib/components/browser/BrowserControlHandoff.svelte";
  import BrowserCaptchaBanner from "$lib/components/browser/BrowserCaptchaBanner.svelte";
  import BrowserFindBar from "$lib/components/browser/BrowserFindBar.svelte";
  import BrowserStartPage from "$lib/components/browser/BrowserStartPage.svelte";
  import {
    createBrowserCompositor,
    registerBrowserCompositor,
    unregisterBrowserCompositor,
    type BrowserCompositor,
  } from "$lib/utils/browserCompositor";
  import {
    BROWSER_FOCUS_URL_EVENT,
  } from "$lib/utils/browserChromeEvents";
  import {
    type BrowserHotkeyAction,
    runBrowserHotkeyAction,
  } from "$lib/utils/browserHotkeys";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { isTauri, shouldUseMobileShell } from "$lib/platform";

  interface Props {
    visible?: boolean;
    workRailVisible?: boolean;
    /** When true, the shell tab strip owns tabs — hide the local tab bar. */
    shellTabChrome?: boolean;
  }

  let { visible = true, workRailVisible = false, shellTabChrome = false }: Props = $props();

  let urlBarFocusNonce = $state(0);
  let panelEl = $state<HTMLElement | null>(null);
  let chromeEl = $state<HTMLElement | null>(null);
  let embedHostEl = $state<HTMLElement | null>(null);
  let compositor = $state<BrowserCompositor | null>(null);

  const useDesktopCompositor = $derived(
    isTauri() && !layout.isMobile && !shouldUseMobileShell(),
  );

  function focusUrlBar() {
    urlBarFocusNonce += 1;
  }

  function handleShellHotkey(action: BrowserHotkeyAction) {
    if (action === "focusUrl") {
      focusUrlBar();
      return;
    }
    runBrowserHotkeyAction(action, humanBrowser);
  }

  onMount(() => {
    if (isTauri() && !shouldUseMobileShell()) {
      compositor = createBrowserCompositor({
        mode: "desktop",
        getActive: () => visible && isTauri() && !layout.isMobile && !shouldUseMobileShell(),
        getShowStartPage: () => humanBrowser.showStartPage,
        getActiveUrl: () => humanBrowser.activeUrl,
        getActiveTabId: () => humanBrowser.activeTab?.id ?? null,
      });
      registerBrowserCompositor(compositor);
    }

    const onFocusUrl = () => focusUrlBar();

    const onKeydown = (event: KeyboardEvent) => {
      if (layout.desktopSurface !== "web" && !humanBrowser.findOpen) return;

      if (event.key === "Escape" && humanBrowser.loading) {
        event.preventDefault();
        void humanBrowser.stop();
        return;
      }

      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      const key = event.key.toLowerCase();
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      // Core browser hotkeys — work even while the URL bar / find field is focused.
      if (key === "l") {
        event.preventDefault();
        handleShellHotkey("focusUrl");
        return;
      }
      if (key === "f") {
        event.preventDefault();
        handleShellHotkey("find");
        return;
      }
      if (key === "b" && event.shiftKey) {
        event.preventDefault();
        handleShellHotkey("bookmarks");
        return;
      }
      if (key === "t" && event.shiftKey) {
        event.preventDefault();
        handleShellHotkey("reopenTab");
        return;
      }
      if (key === "t") {
        event.preventDefault();
        handleShellHotkey("newTab");
        return;
      }
      if (event.key === "[" || event.key === "]") {
        event.preventDefault();
        handleShellHotkey(event.key === "[" ? "goBack" : "goForward");
        return;
      }
      if (typing && key !== "r") return;

      if (key === "w") {
        event.preventDefault();
        handleShellHotkey("closeTab");
        return;
      }
      if (key === "r") {
        event.preventDefault();
        handleShellHotkey("reload");
      }
    };
    window.addEventListener("keydown", onKeydown);
    window.addEventListener(BROWSER_FOCUS_URL_EVENT, onFocusUrl);

    return () => {
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener(BROWSER_FOCUS_URL_EVENT, onFocusUrl);
      const active = compositor;
      active?.detach();
      if (active) unregisterBrowserCompositor(active);
      compositor = null;
    };
  });

  // Sync only on visible edge — wait for tile host measure so native webview
  // never shows at pre-split full-pane bounds over a sibling pane.
  let wasPanelVisible = false;
  $effect(() => {
    const isVisible = visible;
    if (isVisible && !wasPanelVisible) {
      wasPanelVisible = true;
      void (async () => {
        await tick();
        await new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        });
        const active = compositor;
        if (active) await active.flushLayout();
        await humanBrowser.syncActiveTabToNative();
        if (active) await active.flushLayout();
      })();
      return;
    }
    if (!isVisible) wasPanelVisible = false;
  });

  $effect(() => {
    if (!useDesktopCompositor || !visible || !compositor || !embedHostEl || !chromeEl) return;
    humanBrowser.showStartPage;
    layout.activityWidth;
    layout.activityCollapsed;
    layout.shellSidebarExpanded;
    layout.shellSidebarWidth;
    layout.viewportWidth;
    layout.viewportHeight;
    workRailVisible;
    humanBrowser.findOpen;
    // Split / ownership remounts change the host box — force re-attach path.
    shellTabs.paneCount;
    shellTabs.activeGroupId;
    compositor.attach({
      hostEl: embedHostEl,
      panelEl,
      chromeEl,
    });
  });
</script>

<div
  bind:this={panelEl}
  class="human-browser-panel flex h-full min-h-0 flex-col"
  data-browser-panel
  data-debug-label="browser-panel"
>
  <div
    bind:this={chromeEl}
    class="human-browser-chrome relative z-50 flex w-full shrink-0 flex-col"
    data-debug-label="browser-chrome"
  >
    {#if !shellTabChrome}
      <div data-debug-label="browser-tab-bar">
        <HumanBrowserTabBar />
      </div>
    {/if}
    <div data-debug-label="browser-agent-handoff">
      <BrowserControlHandoff />
    </div>

    <div class="browser-toolbar" data-debug-label="browser-url-row">
      <div class="browser-nav-cluster">
        <button
          type="button"
          class="browser-nav-btn"
          aria-label="Back"
          disabled={!humanBrowser.canGoBack}
          onclick={() => void humanBrowser.goBack()}
        >
          <ArrowLeft size={15} strokeWidth={1.75} />
        </button>
        <button
          type="button"
          class="browser-nav-btn"
          aria-label="Forward"
          disabled={!humanBrowser.canGoForward}
          onclick={() => void humanBrowser.goForward()}
        >
          <ArrowRight size={15} strokeWidth={1.75} />
        </button>
        {#if humanBrowser.loading}
          <button
            type="button"
            class="browser-nav-btn browser-nav-btn--stop"
            aria-label="Stop loading"
            onclick={() => void humanBrowser.stop()}
          >
            <Square size={11} strokeWidth={2.25} />
          </button>
        {:else}
          <button
            type="button"
            class="browser-nav-btn"
            aria-label="Reload"
            onclick={() => void humanBrowser.reload()}
          >
            <RefreshCw size={14} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
      <HumanBrowserUrlBar {urlBarFocusNonce} />
      <BrowserChromeActions />
    </div>

    <BrowserFindBar />
    <BrowserCaptchaBanner compact={true} />

    {#if humanBrowser.loading}
      <div class="browser-loading-bar"></div>
    {/if}
  </div>

  <div
    bind:this={embedHostEl}
    class="human-browser-embed relative min-h-0 flex-1 overflow-hidden"
    data-browser-embed-host
    data-debug-label="browser-embed-host"
  >
    {#if humanBrowser.showStartPage}
      <div class="browser-start-page-host">
        <BrowserStartPage />
      </div>
    {/if}
  </div>
</div>
