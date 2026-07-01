<script lang="ts">
  import { onMount } from "svelte";
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
    type BrowserCompositor,
  } from "$lib/utils/browserCompositor";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { isTauri, shouldUseMobileShell } from "$lib/platform";

  interface Props {
    visible?: boolean;
    workRailVisible?: boolean;
  }

  let { visible = true, workRailVisible = false }: Props = $props();

  let urlBarFocusNonce = $state(0);
  let panelEl = $state<HTMLElement | null>(null);
  let chromeEl = $state<HTMLElement | null>(null);
  let embedHostEl = $state<HTMLElement | null>(null);
  let compositor = $state<BrowserCompositor | null>(null);

  const useDesktopCompositor = $derived(
    isTauri() && !layout.isMobile && !shouldUseMobileShell(),
  );

  onMount(() => {
    if (isTauri() && !shouldUseMobileShell()) {
      compositor = createBrowserCompositor({
        mode: "desktop",
        getActive: () => visible && isTauri() && !layout.isMobile && !shouldUseMobileShell(),
        getShowStartPage: () => humanBrowser.showStartPage,
        getActiveUrl: () => humanBrowser.activeUrl,
      });
    }

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

      if (key === "l") {
        event.preventDefault();
        urlBarFocusNonce += 1;
        return;
      }
      if (key === "f") {
        event.preventDefault();
        humanBrowser.openFindBar();
        return;
      }
      if (mod && event.shiftKey && key === "t") {
        event.preventDefault();
        void humanBrowser.reopenClosedTab();
        return;
      }
      if (event.key === "[" || event.key === "]") {
        event.preventDefault();
        if (event.key === "[") void humanBrowser.goBack();
        else void humanBrowser.goForward();
        return;
      }
      if (typing && key !== "r") return;

      if (key === "t") {
        event.preventDefault();
        void humanBrowser.openTab();
        return;
      }
      if (key === "w") {
        event.preventDefault();
        const active = humanBrowser.activeTab;
        if (active) void humanBrowser.closeTab(active.id);
        return;
      }
      if (key === "r") {
        event.preventDefault();
        void humanBrowser.reload();
      }
    };
    window.addEventListener("keydown", onKeydown);

    return () => {
      window.removeEventListener("keydown", onKeydown);
      compositor?.detach();
      compositor = null;
    };
  });

  $effect(() => {
    if (!useDesktopCompositor || !visible || !compositor || !embedHostEl || !chromeEl) return;
    humanBrowser.showStartPage;
    layout.activityWidth;
    layout.activityCollapsed;
    layout.viewportWidth;
    layout.viewportHeight;
    workRailVisible;
    humanBrowser.findOpen;
    compositor.attach({
      hostEl: embedHostEl,
      panelEl,
      chromeEl,
    });
  });
</script>

<div
  bind:this={panelEl}
  class="flex h-full min-h-0 flex-col bg-surface-950 text-surface-50"
  data-browser-panel
  data-debug-label="browser-panel"
>
  <div
    bind:this={chromeEl}
    class="human-browser-chrome relative z-50 flex w-full shrink-0 flex-col"
    data-debug-label="browser-chrome"
  >
    <div data-debug-label="browser-tab-bar">
      <HumanBrowserTabBar />
    </div>
    <div data-debug-label="browser-agent-handoff">
      <BrowserControlHandoff />
    </div>

    <div
      class="flex shrink-0 items-center gap-2 border-b border-surface-800 px-2 py-1.5"
      data-debug-label="browser-url-row"
    >
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          class="btn btn-icon btn-sm"
          aria-label="Back"
          disabled={!humanBrowser.canGoBack}
          onclick={() => void humanBrowser.goBack()}
        >
          <ArrowLeft size={16} />
        </button>
        <button
          type="button"
          class="btn btn-icon btn-sm"
          aria-label="Forward"
          disabled={!humanBrowser.canGoForward}
          onclick={() => void humanBrowser.goForward()}
        >
          <ArrowRight size={16} />
        </button>
        {#if humanBrowser.loading}
          <button
            type="button"
            class="btn btn-icon btn-sm"
            aria-label="Stop loading"
            onclick={() => void humanBrowser.stop()}
          >
            <Square size={14} fill="currentColor" />
          </button>
        {:else}
          <button
            type="button"
            class="btn btn-icon btn-sm"
            aria-label="Reload"
            onclick={() => void humanBrowser.reload()}
          >
            <RefreshCw size={16} />
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
    class="relative min-h-0 flex-1 overflow-hidden bg-surface-900"
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
