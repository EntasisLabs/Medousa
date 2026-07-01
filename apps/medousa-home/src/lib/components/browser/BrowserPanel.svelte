<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ArrowLeft, ArrowRight, Layers, RefreshCw, Square } from "@lucide/svelte";
  import HumanBrowserUrlBar from "$lib/components/browser/HumanBrowserUrlBar.svelte";
  import BrowserChromeActions from "$lib/components/browser/BrowserChromeActions.svelte";
  import BrowserControlHandoff from "$lib/components/browser/BrowserControlHandoff.svelte";
  import BrowserCaptchaBanner from "$lib/components/browser/BrowserCaptchaBanner.svelte";
  import BrowserFindBar from "$lib/components/browser/BrowserFindBar.svelte";
  import BrowserStartPage from "$lib/components/browser/BrowserStartPage.svelte";
  import BrowserTabSheet from "$lib/components/browser/BrowserTabSheet.svelte";
  import BrowserCompositorDebug from "$lib/components/browser/BrowserCompositorDebug.svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import BrowserWebView from "$lib/components/browser/BrowserWebView.svelte";
  import { canUseNativeBrowserWebview } from "$lib/browserWebview";
  import { humanBrowserSetMobileShellActive } from "$lib/humanBrowser";
  import {
    createBrowserCompositor,
    registerBrowserCompositor,
    type BrowserCompositor,
    type BrowserCompositorState,
  } from "$lib/utils/browserCompositor";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { isMobileBrowserUrlFocused } from "$lib/utils/mobileKeyboardViewport";

  async function waitForLayoutFrame() {
    await tick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }

  function waitForKeyboardSettled(timeoutMs = 1200): Promise<void> {
    const vv = typeof window !== "undefined" ? window.visualViewport : null;
    if (!vv) {
      return new Promise<void>((resolve) => setTimeout(resolve, 120));
    }
    return new Promise<void>((resolve) => {
      const start = Date.now();
      let settleTimer: ReturnType<typeof setTimeout> | null = null;
      const cleanup = () => {
        vv.removeEventListener("resize", onChange);
        vv.removeEventListener("scroll", onChange);
        if (settleTimer) clearTimeout(settleTimer);
      };
      const finish = () => {
        cleanup();
        resolve();
      };
      const onChange = () => {
        if (settleTimer) clearTimeout(settleTimer);
        if (Date.now() - start > timeoutMs) {
          finish();
          return;
        }
        settleTimer = setTimeout(finish, 120);
      };
      vv.addEventListener("resize", onChange);
      vv.addEventListener("scroll", onChange);
      settleTimer = setTimeout(finish, 180);
    });
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
  let compositorState = $state<BrowserCompositorState | null>(null);

  let compositor = $state<BrowserCompositor | null>(null);

  async function refreshEmbedAfterUrlBlur() {
    if (!useNative || !visible || !compositor) return;
    await waitForKeyboardSettled();
    await waitForLayoutFrame();
    compositor.scheduleLayout();
  }

  onMount(() => {
    if (useNative) {
      void humanBrowserSetMobileShellActive(true);
      compositor = createBrowserCompositor({
        mode: "mobile",
        getActive: () => useNative && visible,
        getShowStartPage: () => humanBrowser.showStartPage,
        getUrlBarFocused: () => isMobileBrowserUrlFocused(),
        getActiveUrl: () => humanBrowser.activeUrl,
        onStateChange: (state) => {
          compositorState = state;
        },
      });
      registerBrowserCompositor(compositor);
    }

    const onUrlFocus = () => {
      compositor?.scheduleLayout();
    };
    const onUrlBlur = () => {
      void refreshEmbedAfterUrlBlur();
    };

    if (useNative) {
      window.addEventListener("medousa-browser-url-focus", onUrlFocus);
      window.addEventListener("medousa-browser-url-blur", onUrlBlur);
    }

    return () => {
      if (useNative) {
        window.removeEventListener("medousa-browser-url-focus", onUrlFocus);
        window.removeEventListener("medousa-browser-url-blur", onUrlBlur);
        compositor?.detach();
        registerBrowserCompositor(null);
        compositor = null;
      }
    };
  });

  $effect(() => {
    if (!useNative || !visible || !panelEl || !embedHostEl || !compositor) return;
    humanBrowser.showStartPage;
    layout.viewportWidth;

    compositor.attach({
      hostEl: embedHostEl,
      panelEl,
      bottomChromeEl,
    });
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
        {#if humanBrowser.showStartPage}
          <div class="browser-start-page-host">
            <BrowserStartPage />
          </div>
        {/if}
        <BrowserCompositorDebug state={compositorState} />
      {:else if humanBrowser.activeUrl && humanBrowser.activeUrl !== "about:blank"}
        <BrowserWebView
          bind:this={webView}
          {visible}
          url={humanBrowser.activeUrl}
        />
      {:else if humanBrowser.showStartPage}
        <div class="browser-start-page-host">
          <BrowserStartPage />
        </div>
      {/if}
      {#if humanBrowser.loading}
        <div class="browser-loading-bar"></div>
      {/if}
    </div>

    <div
      bind:this={bottomChromeEl}
      data-browser-bottom-chrome
      class="mobile-browser-bottom-chrome {useNative
        ? 'inset-x-0 border-t-0'
        : 'shrink-0 border-t border-surface-800/80'} bg-surface-950/95 backdrop-blur-md"
    >
      <BrowserCaptchaBanner compact={true} />
      <BrowserControlHandoff />
      <BrowserFindBar />
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
        {#if humanBrowser.loading}
          <button
            type="button"
            class="btn btn-icon btn-sm shrink-0"
            aria-label="Stop loading"
            onclick={() => void humanBrowser.stop()}
          >
            <Square size={14} fill="currentColor" />
          </button>
        {:else}
          <button
            type="button"
            class="btn btn-icon btn-sm shrink-0"
            aria-label="Reload"
            onclick={() => void reloadView()}
          >
            <RefreshCw size={18} />
          </button>
        {/if}
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
