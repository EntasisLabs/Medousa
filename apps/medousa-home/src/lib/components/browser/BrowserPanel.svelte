<script lang="ts">
  /**
   * Mobile Web tab browser surface.
   * Shares `humanBrowser` with desktop; native WKWebView on Tauri desktop
   * (fixed Rust layout), iframe fallback on iOS/Android.
   */
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ArrowLeft, ArrowRight, Globe, RefreshCw } from "@lucide/svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserWebView from "$lib/components/browser/BrowserWebView.svelte";
  import { canUseNativeBrowserWebview } from "$lib/browserWebview";
  import {
    humanBrowserEmbedApplyMobileLayout,
    humanBrowserEmbedHide,
    type HumanBrowserNavigatedPayload,
  } from "$lib/humanBrowser";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { readMobileBottomChromeHeight, logMobileBrowserLayoutDebug } from "$lib/utils/mobileBrowserLayout";

  interface Props {
    visible?: boolean;
  }

  let { visible = true }: Props = $props();

  const useNative = canUseNativeBrowserWebview();

  let webView = $state<{
    reload: () => Promise<void>;
    goBack: () => Promise<void>;
    goForward: () => Promise<void>;
  } | null>(null);

  async function presentEmbed() {
    if (!useNative || !visible) return;
    await humanBrowserEmbedApplyMobileLayout({
      bottomChromeHeight: readMobileBottomChromeHeight(),
    });
    requestAnimationFrame(() => logMobileBrowserLayoutDebug());
  }

  $effect(() => {
    if (!useNative || !visible) return;
    layout.viewportWidth;
    void presentEmbed();
    return () => {
      void humanBrowserEmbedHide();
    };
  });

  onMount(() => {
    const unlisteners: Promise<() => void>[] = [];
    unlisteners.push(
      listen<HumanBrowserNavigatedPayload>("human-browser-navigated", (event) => {
        humanBrowser.syncFromNative(event.payload);
      }),
    );

    const onResize = () => {
      void presentEmbed();
    };
    if (useNative) {
      window.addEventListener("resize", onResize);
    }

    return () => {
      if (useNative) window.removeEventListener("resize", onResize);
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });

  async function reloadView() {
    if (useNative) {
      await humanBrowser.reload();
      return;
    }
    await webView?.reload();
  }
</script>

{#if visible}
  <div class="flex h-full min-h-0 flex-col bg-surface-950">
    <!-- Fixed toolbar height — must match MOBILE_BROWSER_TOOLBAR_HEIGHT in human_browser.rs -->
    <div
      class="flex h-[52px] shrink-0 items-center gap-2 overflow-hidden border-b border-surface-800 px-3"
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
        <button
          type="button"
          class="btn btn-icon btn-sm"
          aria-label="Reload"
          onclick={() => void reloadView()}
        >
          <RefreshCw size={16} />
        </button>
      </div>
      <HumanBrowserUrlBar />
    </div>

    <div
      data-browser-surface
      class="relative min-h-0 flex-1 overflow-hidden bg-surface-950"
    >
      {#if useNative}
        {#if humanBrowser.activeUrl === "about:blank"}
          <div
            class="flex h-full min-h-0 flex-col items-center justify-center gap-3 bg-surface-900 text-surface-300"
          >
            <Globe size={40} strokeWidth={1.25} />
            <p class="text-sm">Enter a URL above to start browsing.</p>
          </div>
        {/if}
      {:else if humanBrowser.activeUrl && humanBrowser.activeUrl !== "about:blank"}
        <BrowserWebView
          bind:this={webView}
          {visible}
          url={humanBrowser.activeUrl}
        />
      {:else}
        <div
          class="flex h-full min-h-0 flex-col items-center justify-center gap-3 bg-surface-900 text-surface-300"
        >
          <Globe size={40} strokeWidth={1.25} />
          <p class="text-sm">Enter a URL above to start browsing.</p>
        </div>
      {/if}
      {#if humanBrowser.loading}
        <div class="pointer-events-none absolute inset-x-0 top-0 h-0.5 bg-primary-500/80"></div>
      {/if}
    </div>
  </div>
{/if}
