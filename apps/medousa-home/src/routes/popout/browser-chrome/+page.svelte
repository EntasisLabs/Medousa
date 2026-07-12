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
  import { humanBrowserActivateTab } from "$lib/humanBrowser";

  let urlBarFocusNonce = $state(0);

  $effect(() => {
    const title = humanBrowserPopout.scopeLabel;
    void setBrowserWindowTitle(title === "Web" ? "Medousa Web" : title);
  });

  function syncPopoutContent() {
    const active = humanBrowserPopout.activeTab;
    if (!active) return;
    void humanBrowserActivateTab(active.id, active.url);
    void humanBrowserPopout.refreshNativeNavState();
  }

  onMount(() => {
    settings.applyTheme();

    const stopListeners = attachHumanBrowserSurface(humanBrowserPopout, "popout");
    syncPopoutContent();

    const unlisteners: Promise<() => void>[] = [];
    if (isTauri()) {
      unlisteners.push(
        listen<boolean>("browser-window-visibility", (event) => {
          if (event.payload) syncPopoutContent();
        }),
      );
    }

    const onKeydown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      const key = event.key.toLowerCase();
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      if (key === "l") {
        event.preventDefault();
        urlBarFocusNonce += 1;
        return;
      }
      if (typing) return;

      if (key === "t") {
        event.preventDefault();
        void humanBrowserPopout.openTab();
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
    window.addEventListener("keydown", onKeydown);

    return () => {
      window.removeEventListener("keydown", onKeydown);
      stopListeners();
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });

  async function handleClose() {
    if (isTauri()) await hideBrowser();
  }
</script>

<!-- Fixed-height chrome strip — height must stay in sync with CHROME_HEIGHT_LOGICAL in human_browser.rs -->
<div class="human-browser-chrome relative z-50 flex h-[132px] w-full flex-col overflow-hidden">
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

  {#if humanBrowserPopout.loading}
    <div class="browser-loading-bar"></div>
  {/if}
</div>
