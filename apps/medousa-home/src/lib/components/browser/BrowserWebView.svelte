<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    canUseNativeBrowserWebview,
    hideNativeBrowserWebview,
    measureBrowserWebviewBounds,
    nativeBrowserGoBack,
    nativeBrowserGoForward,
    navigateNativeBrowserWebview,
    reloadNativeBrowserWebview,
    syncNativeBrowserWebview,
  } from "$lib/browserWebview";
  import { layout } from "$lib/stores/layout.svelte";
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    visible?: boolean;
    url: string;
    /** Element that defines the browser surface (content pane). */
    measureEl?: HTMLElement | null;
    /** Bumped when layout chrome changes (activity rail, viewport). */
    boundsSyncKey?: string;
  }

  let { visible = true, url, measureEl = null, boundsSyncKey = "" }: Props = $props();

  let useNative = $state(false);
  let syncFrame = 0;
  let mountedUrl = $state<string | null>(null);

  function scheduleSyncBounds() {
    if (typeof window === "undefined") return;
    const frame = ++syncFrame;
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        if (frame !== syncFrame) return;
        void syncBounds();
      });
    });
  }

  async function syncBounds() {
    if (!measureEl || !visible || !useNative) {
      if (useNative) await hideNativeBrowserWebview();
      return;
    }
    const bounds = measureBrowserWebviewBounds(measureEl);
    if (bounds.width < 8 || bounds.height < 8) {
      await hideNativeBrowserWebview();
      return;
    }
    const show = Boolean(url && url !== "about:blank");
    await syncNativeBrowserWebview(bounds, show, show ? url : null);
  }

  onMount(() => {
    useNative = canUseNativeBrowserWebview();

    let stopNativeNav: (() => void) | undefined;
    if (useNative) {
      void listen<string>("browser-native-navigated", (event) => {
        void browser.syncFromNative(event.payload);
      }).then((unlisten) => {
        stopNativeNav = unlisten;
      });
    }

    return () => {
      stopNativeNav?.();
      void hideNativeBrowserWebview();
    };
  });

  $effect(() => {
    const el = measureEl;
    if (!el) return;

    const observer = new ResizeObserver(() => scheduleSyncBounds());
    observer.observe(el);

    const main = document.querySelector(".workshop-main");
    if (main) observer.observe(main);

    const onLayout = () => scheduleSyncBounds();
    window.addEventListener("scroll", onLayout, true);
    window.addEventListener("resize", onLayout);
    window.visualViewport?.addEventListener("resize", onLayout);
    window.visualViewport?.addEventListener("scroll", onLayout);

    scheduleSyncBounds();

    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", onLayout, true);
      window.removeEventListener("resize", onLayout);
      window.visualViewport?.removeEventListener("resize", onLayout);
      window.visualViewport?.removeEventListener("scroll", onLayout);
    };
  });

  $effect(() => {
    measureEl;
    url;
    visible;
    boundsSyncKey;
    layout.activityWidth;
    layout.activityCollapsed;
    layout.viewportWidth;
    scheduleSyncBounds();
  });

  $effect(() => {
    if (!useNative || !visible || !url || url === "about:blank") return;
    if (mountedUrl === url) return;
    mountedUrl = url;
    void navigateNativeBrowserWebview(url);
  });

  export async function reload() {
    if (useNative) {
      await reloadNativeBrowserWebview();
      return;
    }
    if (iframeEl) iframeEl.src = url;
  }

  export async function goBack() {
    if (useNative) {
      await nativeBrowserGoBack();
      return;
    }
    browser.goBack();
  }

  export async function goForward() {
    if (useNative) {
      await nativeBrowserGoForward();
      return;
    }
    browser.goForward();
  }

  let iframeEl = $state<HTMLIFrameElement | null>(null);
</script>

<div class="h-full w-full">
  {#if !useNative && url && url !== "about:blank"}
    <iframe
      bind:this={iframeEl}
      title="Web browser"
      src={url}
      class="block h-full w-full border-0 bg-white"
      sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox"
    ></iframe>
  {:else if useNative}
    <div class="h-full w-full" aria-hidden="true"></div>
  {/if}
</div>
