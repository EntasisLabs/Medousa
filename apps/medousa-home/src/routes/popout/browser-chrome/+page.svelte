<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowLeft, ArrowRight, RefreshCw, Square, X } from "@lucide/svelte";
  import HumanBrowserTabBar from "$lib/components/browser/HumanBrowserTabBar.svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import { humanBrowserPopout } from "$lib/stores/humanBrowser.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { hideBrowser, isTauri, setBrowserWindowTitle } from "$lib/window";
  import { attachHumanBrowserSurface } from "$lib/utils/humanBrowserListeners";
  import { BROWSER_FOCUS_URL_EVENT } from "$lib/utils/browserChromeEvents";
  import BrowserNavigationError from "$lib/components/browser/BrowserNavigationError.svelte";
  import { humanBrowserPopoutSetChromeOverlay } from "$lib/humanBrowser";
  import { getBrowserPopoverOverlayDepth, setBrowserPopoverLayoutPort } from "$lib/utils/browserPopoverOverlay";

  let urlBarFocusNonce = $state(0);
  let visible = $state(true);

  $effect(() => {
    if (!visible) return;
    humanBrowserPopout.activeTab?.id;
    return humanBrowserPopout.watchActiveNavigation();
  });

  $effect(() => {
    const title = humanBrowserPopout.scopeLabel;
    void setBrowserWindowTitle(title === "Web" ? "Medousa Web" : title);
  });

  function syncPopoutContent() {
    void humanBrowserPopout.syncActiveTabToNative();
  }

  onMount(() => {
    settings.applyTheme();

    // This window has its own native chrome view, rather than the Home compositor.
    // Grow it while menus are open so DOM popovers can extend past the toolbar.
    let overlayChain: Promise<void> = Promise.resolve();
    const syncOverlay = () => {
      const active = getBrowserPopoverOverlayDepth() > 0;
      overlayChain = overlayChain.catch(() => {}).then(() => humanBrowserPopoutSetChromeOverlay(active));
      return overlayChain;
    };
    if (isTauri()) {
      setBrowserPopoverLayoutPort({
        scheduleLayout: () => { void syncOverlay().catch(console.error); },
        flushLayout: syncOverlay,
      });
      void syncOverlay().catch(console.error);
    }

    const stopListeners = attachHumanBrowserSurface(humanBrowserPopout, "popout");
    syncPopoutContent();

    const unlisteners: Promise<() => void>[] = [];
    if (isTauri()) {
      unlisteners.push(
        listen<boolean>("browser-window-visibility", (event) => {
          visible = event.payload;
          if (visible) syncPopoutContent();
        }),
      );
    }

    const onKeydown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      const key = event.key.toLowerCase();

      if (key === "l") {
        event.preventDefault();
        urlBarFocusNonce += 1;
        return;
      }
      if (event.key === "[" || event.key === "]") {
        event.preventDefault();
        if (event.key === "[") void humanBrowserPopout.goBack();
        else void humanBrowserPopout.goForward();
        return;
      }

      if (key === "t") {
        event.preventDefault();
        if (event.shiftKey) void humanBrowserPopout.reopenClosedTab();
        else void humanBrowserPopout.openTab();
        return;
      }
      if (key === "w") {
        event.preventDefault();
        const tab = humanBrowserPopout.activeTab;
        if (tab) void humanBrowserPopout.closeTab(tab.id);
        return;
      }
      if (key === "r") {
        event.preventDefault();
        void humanBrowserPopout.reload();
      }
    };
    const focusUrl = () => { urlBarFocusNonce += 1; };
    window.addEventListener(BROWSER_FOCUS_URL_EVENT, focusUrl);
    window.addEventListener("keydown", onKeydown);

    return () => {
      setBrowserPopoverLayoutPort(null);
      if (isTauri()) {
        void overlayChain.catch(() => {}).then(() => humanBrowserPopoutSetChromeOverlay(false)).catch(console.error);
      }
      window.removeEventListener(BROWSER_FOCUS_URL_EVENT, focusUrl);
      window.removeEventListener("keydown", onKeydown);
      stopListeners();
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });

  async function handleClose() {
    if (isTauri()) await hideBrowser();
  }
</script>

<!-- Toolbar height stays fixed; the native chrome view grows for floating menus. -->
<div class="human-browser-chrome relative z-50 flex h-[132px] w-full flex-col">
  <HumanBrowserTabBar />

  <div class="browser-toolbar">
    <div class="browser-nav-cluster">
      <button
        type="button"
        class="browser-nav-btn"
        aria-label="Back"
        disabled={!humanBrowserPopout.canGoBack}
        onclick={() => void humanBrowserPopout.goBack()}
      >
        <ArrowLeft size={16} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="browser-nav-btn"
        aria-label="Forward"
        disabled={!humanBrowserPopout.canGoForward}
        onclick={() => void humanBrowserPopout.goForward()}
      >
        <ArrowRight size={16} strokeWidth={1.75} />
      </button>
      <BrowserNavigationError store={humanBrowserPopout} />

  {#if humanBrowserPopout.loading}
        <button
          type="button"
          class="browser-nav-btn browser-nav-btn--stop"
          aria-label="Stop loading"
          onclick={() => void humanBrowserPopout.stop()}
        >
          <Square size={12} strokeWidth={2.25} />
        </button>
      {:else}
        <button
          type="button"
          class="browser-nav-btn"
          aria-label="Reload"
          onclick={() => void humanBrowserPopout.reload()}
        >
          <RefreshCw size={15} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
    <HumanBrowserUrlBar {urlBarFocusNonce} />
    <BrowserChromeActions />
    {#if isTauri()}
      <button
        type="button"
        class="browser-chrome-btn shrink-0"
        aria-label="Close browser"
        title="Close"
        onclick={handleClose}
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    {/if}
  </div>

  <BrowserNavigationError store={humanBrowserPopout} />

  {#if humanBrowserPopout.loading}
    <div class="browser-loading-bar"></div>
  {/if}
</div>
