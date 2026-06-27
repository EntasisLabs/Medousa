<script lang="ts">
  /**
   * Mobile Web tab — embed height = panel height − chrome block + chrome padding-top.
   */
  import { onMount, tick } from "svelte";
  import { ArrowLeft, ArrowRight, Globe, Layers } from "@lucide/svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import BrowserControlHandoff from "$lib/components/browser/BrowserControlHandoff.svelte";
  import BrowserCaptchaBanner from "$lib/components/browser/BrowserCaptchaBanner.svelte";
  import BrowserTabSheet from "$lib/components/browser/BrowserTabSheet.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import BrowserWebView from "$lib/components/browser/BrowserWebView.svelte";
  import { canUseNativeBrowserWebview } from "$lib/browserWebview";
  import {
    humanBrowserEmbedApplyMobileLayout,
    humanBrowserEmbedHide,
    humanBrowserEmbedReadBounds,
    humanBrowserEmbedShow,
    humanBrowserSetMobileShellActive,
  } from "$lib/humanBrowser";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import {
    readMobileBottomChromeHeight,
    measureEmbedHostBounds,
    measureMobileBrowserEmbedBounds,
    computeMobileBrowserEmbedMetrics,
    isMobileBottomChromeMeasured,
  } from "$lib/utils/mobileBrowserLayout";


  async function waitForLayoutFrame() {
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }

  let embedChain: Promise<void> = Promise.resolve();
  let embedGeneration = 0;

  function isWorkshopReadback(readback: { y: number } | null): boolean {
    return readback != null && readback.y >= 100;
  }

  function embedTargetBottomPx(): number | null {
    if (!panelEl) return null;
    const metrics = computeMobileBrowserEmbedMetrics(panelEl, bottomChromeEl);
    return metrics ? metrics.bounds.y + metrics.bounds.height : null;
  }

  async function waitForEmbedBounds(
    hostEl: HTMLElement | null,
    panel: HTMLElement | null,
    chrome: HTMLElement | null,
    maxAttempts = 24,
  ): Promise<ReturnType<typeof measureMobileBrowserEmbedBounds>> {
    for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
      if (!panel || !isMobileBottomChromeMeasured()) {
        await waitForLayoutFrame();
        continue;
      }
      if (useNative) {
        const chromeReady =
          chrome instanceof HTMLElement &&
          chrome.getBoundingClientRect().height >= 8;
        if (!chromeReady) {
          await waitForLayoutFrame();
          continue;
        }
      }
      const bounds = useNative
        ? measureMobileBrowserEmbedBounds(panel, chrome)
        : measureEmbedHostBounds(hostEl);
      if (bounds) return bounds;
      await waitForLayoutFrame();
    }
    return null;
  }

  async function applyEmbedOnce(): Promise<void> {
    if (!useNative || !visible) return;
    const gen = ++embedGeneration;

    await waitForLayoutFrame();

    let contentBounds = await waitForEmbedBounds(
      embedHostEl,
      panelEl,
      bottomChromeEl,
    );
    if (!contentBounds) {
      return;
    }
    if (gen !== embedGeneration) return;

    await humanBrowserSetMobileShellActive(true);
    if (gen !== embedGeneration) return;

    let recreated = await humanBrowserEmbedApplyMobileLayout({
      bottomChromeHeight: readMobileBottomChromeHeight(),
      contentBounds,
    });
    await humanBrowserEmbedShow();
    if (gen !== embedGeneration) return;

    let readback = await humanBrowserEmbedReadBounds().catch(() => null);
    const targetBottom = embedTargetBottomPx();
    let gap =
      readback && targetBottom != null
        ? targetBottom - (readback.y + readback.height)
        : null;

    const hostMismatch =
      panelEl &&
      readback &&
      contentBounds &&
      (Math.abs(readback.y - contentBounds.y) > 2 ||
        Math.abs(readback.height - contentBounds.height) > 2);
    const workshopStomp = isWorkshopReadback(readback);

    if (gap != null && (gap > 2 || workshopStomp || hostMismatch)) {
      await waitForLayoutFrame();
      if (gen !== embedGeneration) return;
      contentBounds = await waitForEmbedBounds(
        embedHostEl,
        panelEl,
        bottomChromeEl,
      );
      if (contentBounds) {
        await humanBrowserSetMobileShellActive(true);
        recreated = await humanBrowserEmbedApplyMobileLayout({
          bottomChromeHeight: readMobileBottomChromeHeight(),
          contentBounds,
        });
        await humanBrowserEmbedShow();
        readback = await humanBrowserEmbedReadBounds().catch(() => null);
        gap =
          readback && embedTargetBottomPx() != null
            ? embedTargetBottomPx()! - (readback.y + readback.height)
            : null;
      }
    }

    const url = humanBrowser.activeUrl;
    if (gen !== embedGeneration) return;
    if (recreated && url && url !== "about:blank") {
      await humanBrowser.navigate(url);
    }
  }

  let embedRaf: number | null = null;

  function scheduleEmbed() {
    if (embedRaf != null) cancelAnimationFrame(embedRaf);
    embedRaf = requestAnimationFrame(() => {
      embedRaf = null;
      embedChain = embedChain.then(() => applyEmbedOnce()).catch(() => {});
    });
  }

  async function presentEmbed() {
    scheduleEmbed();
  }

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

  let panelEl = $state<HTMLElement | null>(null);
  let embedHostEl = $state<HTMLElement | null>(null);
  let bottomChromeEl = $state<HTMLElement | null>(null);
  let resizeObserver: ResizeObserver | null = null;

  $effect(() => {
    if (!useNative || !visible) return;
    return () => {
      void humanBrowserEmbedHide();
    };
  });

  $effect(() => {
    if (!useNative || !visible) return;
    layout.viewportWidth;
    void presentEmbed();
  });

  $effect(() => {
    if (!useNative || !visible || !panelEl) return;

    resizeObserver?.disconnect();
    resizeObserver = new ResizeObserver(() => {
      scheduleEmbed();
    });
    resizeObserver.observe(panelEl);
    const bottomChrome = document.querySelector("[data-browser-bottom-chrome]");
    if (bottomChrome instanceof HTMLElement) {
      resizeObserver.observe(bottomChrome);
    }

    return () => {
      resizeObserver?.disconnect();
      resizeObserver = null;
    };
  });

  onMount(() => {
    void humanBrowserSetMobileShellActive(true);

    const onResize = () => {
      void presentEmbed();
    };
    if (useNative) {
      window.addEventListener("resize", onResize);
    }

    return () => {
      if (useNative) window.removeEventListener("resize", onResize);
      if (embedRaf != null) cancelAnimationFrame(embedRaf);
      resizeObserver?.disconnect();
    };
  });

  async function reloadView() {
    if (useNative) {
      await humanBrowser.reload();
      return;
    }
    await webView?.reload();
  }

  let tabsOpen = $state(false);
  let tabsAnchorRect = $state<DOMRect | null>(null);
  let tabsAnchorEl = $state<HTMLButtonElement | null>(null);
  let toastMessage = $state<string | null>(null);
  let toastActionLabel = $state<string | undefined>(undefined);
  let toastAction = $state<(() => void) | undefined>(undefined);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function showMobileToast(message: string, actionLabel?: string, onAction?: () => void) {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = message;
    toastActionLabel = actionLabel;
    toastAction = onAction;
    toastTimer = setTimeout(dismissMobileToast, 4000);
  }

  function dismissMobileToast() {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = null;
    toastActionLabel = undefined;
    toastAction = undefined;
  }
</script>

{#if visible}
  <div
    bind:this={panelEl}
    data-browser-panel
    class="relative flex h-full min-h-0 flex-col overflow-hidden bg-surface-950"
  >
    <div
      bind:this={embedHostEl}
      data-browser-embed-host
      class="{useNative
        ? 'absolute inset-0 overflow-hidden bg-transparent'
        : 'relative min-h-0 flex-1 overflow-hidden bg-surface-950'}"
    >
      {#if useNative}
        {#if humanBrowser.activeUrl === "about:blank"}
          <div
            class="flex h-full min-h-0 flex-col items-center justify-center gap-3 bg-surface-900 text-surface-300"
          >
            <Globe size={40} strokeWidth={1.25} />
            <p class="text-sm">Enter a URL below to start browsing.</p>
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
          <p class="text-sm">Enter a URL below to start browsing.</p>
        </div>
      {/if}
      {#if humanBrowser.loading}
        <div class="pointer-events-none absolute inset-x-0 top-0 h-0.5 bg-primary-500/80"></div>
      {/if}
    </div>

    <div
      bind:this={bottomChromeEl}
      data-browser-bottom-chrome
      class="mobile-browser-bottom-chrome {useNative
        ? 'absolute inset-x-0 bottom-0 z-20 border-t-0'
        : 'shrink-0 border-t border-surface-800/80'} bg-surface-950/95 backdrop-blur-md"
    >
      <BrowserCaptchaBanner compact={true} />
      <div class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-800/80 px-2 py-1">
        <BrowserControlHandoff compact={true} />
      </div>
      <div data-browser-controls class="flex items-center gap-1 overflow-x-auto">
        <button
          bind:this={tabsAnchorEl}
          type="button"
          class="btn btn-icon btn-sm shrink-0"
          aria-label="Tabs"
          title="Tabs"
          data-browser-popover-trigger
          aria-expanded={tabsOpen}
          onclick={(event) => {
            event.stopPropagation();
            if (!tabsOpen) {
              tabsAnchorRect = tabsAnchorEl?.getBoundingClientRect() ?? null;
            }
            tabsOpen = !tabsOpen;
          }}
        >
          <Layers size={18} />
        </button>
        <button
          type="button"
          class="btn btn-icon btn-sm shrink-0"
          aria-label="Back"
          disabled={!humanBrowser.canGoBack}
          onclick={() => void humanBrowser.goBack()}
        >
          <ArrowLeft size={18} />
        </button>
        <button
          type="button"
          class="btn btn-icon btn-sm shrink-0"
          aria-label="Forward"
          disabled={!humanBrowser.canGoForward}
          onclick={() => void humanBrowser.goForward()}
        >
          <ArrowRight size={18} />
        </button>
        <div class="flex min-w-0 flex-1 items-center gap-1">
          <HumanBrowserUrlBar mobile />
          <BrowserChromeActions
            mobile
            onReload={() => reloadView()}
            onMobileToast={(message, actionLabel, onAction) =>
              showMobileToast(message, actionLabel, onAction)}
          />
        </div>
      </div>
    </div>

    <BrowserTabSheet
      open={tabsOpen}
      anchorRect={tabsAnchorRect}
      mobile
      onClose={() => (tabsOpen = false)}
    />
    <MobileToast
      message={toastMessage}
      actionLabel={toastActionLabel}
      onAction={toastAction}
      onDismiss={dismissMobileToast}
    />
  </div>
{/if}
